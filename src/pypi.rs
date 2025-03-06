//! This module contains all the logic for adding PyPI packages to a lock file.

use std::collections::HashMap;
use std::env;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;

use indexmap::IndexMap;
use pep508_rs::{PackageName, Requirement};
use rattler_conda_types::{PackageRecord, Platform};
use rattler_lock::{
    LockFileBuilder, PackageHashes, PypiIndexes, PypiPackageData, PypiPackageEnvironmentData,
    UrlOrPath,
};

use rattler_pypi_interop::index::{
    ArtifactRequest, CheckAvailablePackages, PackageDb, PackageSourcesBuilder,
};
use rattler_pypi_interop::python_env::{find_distributions_in_venv, Distribution};
use rattler_pypi_interop::types::{
    ArtifactInfo, ArtifactName, InstallPaths, NormalizedPackageName, PypiVersion, WheelCoreMetadata,
};

use reqwest::Client;
use reqwest_middleware::ClientWithMiddleware;
use tokio::runtime::Runtime;
use url::Url;

#[derive(Debug, Clone, serde::Serialize)]
/// Used to store both information about the distribution and the metadata of the wheel
pub struct PythonPackage {
    /// The distribution information
    distribution: Distribution,
    /// The metadata of the wheel found in the `METADATA` file of the Python package
    metadata: WheelCoreMetadata,
}

impl PythonPackage {
    /// Using `prefix` and `distribution`, where `prefix` is the installation prefix of the
    /// Python package (e.g. `/opt/environment`), returns a new [`PythonPackage`] struct.
    ///
    /// # Errors
    ///
    /// This function will error if we are unable to find or parse the "METADATA" file
    /// that should be included in the Python package's `.dist-info` folder.
    ///
    pub fn new(prefix: &Path, distribution: &Distribution) -> miette::Result<Self> {
        let path_to_metadata = prefix.join(&distribution.dist_info).join("METADATA");

        let metadata_bytes = std::fs::read(&path_to_metadata)
            .map_err(|err| miette::miette!("failed to read metadata file: {:?}", err))?;

        let metadata = WheelCoreMetadata::try_from(metadata_bytes.as_slice())
            .map_err(|err| miette::miette!("failed to parse metadata: {:?}", err))?;

        Ok(Self {
            distribution: distribution.clone(),
            metadata,
        })
    }
}

/// Normalize url according to pip standards
fn normalize_index_url(mut url: Url) -> Url {
    let path = url.path();
    if !path.ends_with('/') {
        url.set_path(&format!("{path}/"));
    }
    url
}

/// Get all the PyPI packages for a provided prefix
pub fn get_pypi_packages(
    prefix: &str,
    python_package: &PackageRecord,
) -> miette::Result<Vec<PythonPackage>> {
    let prefix_path = Path::new(prefix);
    let is_windows = env::consts::OS == "windows";
    if let Some((major, minor)) = python_package.version.as_major_minor() {
        let version_components = (major as u32, minor as u32, 0);
        let install_paths = InstallPaths::for_venv(version_components, is_windows);

        let distributions = find_distributions_in_venv(prefix_path, &install_paths)
            .map_err(|err| miette::miette!("failed to find distributions in venv: {:?}", err))?;

        distributions
            .iter()
            .filter(|dist| dist.installer.as_ref().unwrap_or(&String::new()) == "pip")
            .map(|dist| PythonPackage::new(prefix_path, dist))
            .collect()
    } else {
        Err(miette::miette!("Could not determine Python version"))
    }
}

/// Returns a new PackageDb instance for the PyPI we use in this project
pub fn get_package_db(index_url: &str) -> miette::Result<PackageDb> {
    let index_url = normalize_index_url(
        Url::parse(index_url)
            .map_err(|err| miette::miette!("failed to parse index URL: {:?}", err))?,
    );
    let sources = PackageSourcesBuilder::new(index_url).build()?;

    let cache_dir = dirs::cache_dir()
        .ok_or_else(|| miette::miette!("failed to determine cache directory"))?
        .join("rattler/pypi");

    let client = ClientWithMiddleware::from(Client::new());

    PackageDb::new(sources, client, &cache_dir, CheckAvailablePackages::Always)
}

/// Used to retrieve information from the index for the provided packages
pub fn get_available_artifacts(
    package_db: &PackageDb,
    packages: &Vec<PythonPackage>,
) -> miette::Result<HashMap<NormalizedPackageName, Arc<ArtifactInfo>>> {
    let mut artifacts = HashMap::new();

    let runtime = Runtime::new()
        .map_err(|err| miette::miette!("Could not acquire tokio runtime: {:?}", err))?;

    let result = runtime.block_on(async {
        for package in packages {
            let request = ArtifactRequest::FromIndex(package.distribution.name.clone());
            let available_artifacts = package_db.available_artifacts(request).await?;

            let artifact_name = package.distribution.name.clone();
            if let Ok(matching_artifact) =
                match_distribution_with_artifact(package.distribution.clone(), available_artifacts)
            {
                artifacts.insert(artifact_name, matching_artifact);
            } else {
                return Err(miette::miette!(
                    "Unable to generate lock file because of missing information. Environment is most likely corrupted."
                ));
            }
        }

        Ok(())
    });

    result?;

    Ok(artifacts)
}

/// A function that matches information provided as a [`Distribution`] struct with the data returned
/// from the [`PackageDb::available_artifacts`] method. An error is returned if an [`ArtifactInfo`]
/// cannot be found.
///
/// # Arguments
///
/// * `distribution` a [`Distribution`] struct which is generated by reading the file system
/// * `artifacts` an [`IndexMap`] containing [`ArtifactInfo`] structs retrieved by [`PackageDb::available_artifacts`]
///
pub fn match_distribution_with_artifact(
    distribution: Distribution,
    artifacts: &IndexMap<PypiVersion, Vec<Arc<ArtifactInfo>>>,
) -> miette::Result<Arc<ArtifactInfo>> {
    let version = distribution.version.clone();
    let dist_version = PypiVersion::Version {
        version: version.clone(),
        package_allows_prerelease: version.any_prerelease(),
    };

    if let Some(matching_artifacts) = artifacts.get(&dist_version) {
        for matchng_art in matching_artifacts {
            let filename = matchng_art.filename.clone();
            if let ArtifactName::Wheel(_) = filename {
                return Ok(matchng_art.clone());
            }
        }
    }

    Err(miette::miette!(
        "no artifacts found for version: {:?}",
        distribution.version
    ))
}

/// Add PyPI packages to lock file
///
/// This function retrieves available artifacts for the provided packages from the specified
/// PyPI index URL and adds them to the lock file. It is essentially a wrapper over the
/// [`rattler_lock::LockFileBuilder::add_pypi_package`] allowing multiple packages to
/// be added at once.
///
/// # Arguments
///
/// * `lock_file` - A mutable reference to the `LockFileBuilder` where the packages will be added.
/// * `environment` - A string slice representing the environment name.
/// * `platform` - The platform for which the packages are being added.
/// * `packages` - A vector of `PythonPackage` structs representing the packages to be added.
/// * `index_url` - A string slice representing the URL of the PyPI index (e.g. "https://pypi.org/simple").
/// * `platform` - A [`Platform`] enum which represents the subdir the package is installed into (e.g. "linux-64").
///
/// # Examples
///
/// ```rust
/// use rattler_lock::LockFileBuilder;
/// use rattler_conda_types::Platform;
/// use std::path::Path;
///
/// let mut lock_file = LockFileBuilder::new();
/// let environment = "default";
/// let platform = Platform::NoArch;
/// let packages = vec![]; // Replace with actual packages
/// let index_url = "https://pypi.org/simple";
///
/// add_pypi_packages(&mut lock_file, environment, platform, packages, index_url).unwrap();
/// ```
pub fn add_pypi_packages(
    lock_file: &mut LockFileBuilder,
    environment: &str,
    packages: Vec<PythonPackage>,
    index_url: &str,
    platform: Platform,
) -> miette::Result<()> {
    let package_db = get_package_db(index_url)?;
    let artifacts = get_available_artifacts(&package_db, &packages)?;

    if !packages.is_empty() {
        let indexes = PypiIndexes {
            indexes: vec![
                Url::parse(index_url).map_err(|_err| miette::miette!("Cannot parse PyPI URL"))?
            ],
            find_links: vec![],
        };
        lock_file.set_pypi_indexes(environment, indexes);
    }

    for package in packages {
        let name = &package.distribution.name;

        if let Some(artifact) = artifacts.get(name) {
            lock_file.add_pypi_package(
                environment,
                platform,
                PypiPackageData {
                    name: PackageName::new(name.to_string())
                        .map_err(|e| miette::miette!("Failed to create package name: {:?}", e))?,
                    version: package.distribution.version.clone(),
                    location: UrlOrPath::Url(artifact.url.clone()),
                    hash: artifact
                        .hashes
                        .as_ref()
                        .and_then(|hashes| hashes.sha256)
                        .map(PackageHashes::Sha256),
                    requires_dist: package
                        .metadata
                        .requires_dist
                        .iter()
                        .map(|req| {
                            Requirement::from_str(&req.to_string()).map_err(|e| {
                                miette::miette!("Failed to parse requirement: {:?}", e)
                            })
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                    requires_python: artifact.requires_python.clone(),
                    editable: false,
                },
                PypiPackageEnvironmentData::default(),
            );
        } else {
            return Err(miette::miette!(
                "Cannot find artifact information for Python package."
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use std::path::Path;

    use crate::conda::get_conda_packages;
    use crate::get_python_package;

    use super::*;

    static PYTHON_VERSION: (u32, u32, u32) = (3, 13, 0);

    #[test]
    fn test_normalize_index_url() {
        let url = Url::parse("https://pypi.org").unwrap();
        let normalized_url = normalize_index_url(url);
        assert_eq!(normalized_url.as_str(), "https://pypi.org/");
    }

    #[test]
    fn test_get_pypi_packages() {
        let binding = Path::new(env!("CARGO_MANIFEST_DIR")).join("test-data/test-install-prefix");
        let prefix = binding.to_str().unwrap();
        let conda_packages = get_conda_packages(prefix);
        let python_package = get_python_package(&conda_packages).unwrap();
        let packages =
            get_pypi_packages(prefix, &python_package.repodata_record.package_record).unwrap();

        let distributions = packages
            .iter()
            .map(|p| p.distribution.clone())
            .collect::<Vec<_>>();
        insta::assert_ron_snapshot!(distributions);
    }

    #[test]
    #[should_panic(expected = "failed to parse metadata: FailedToParseMetadata")]
    /// Make sure that the [`get_pypi_packages`] function errors when trying to read corrupted `METADATA` file.
    ///
    /// Corrupted data file:
    ///
    /// * `test-data/test-install-prefix-corrupted/lib/python3.13/site-packages/blinker-1.9.0.dist-info/METADATA`.
    fn test_get_pypi_packages_with_errors() {
        let binding =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("test-data/test-install-prefix-corrupted");
        let prefix = binding.to_str().unwrap();
        let conda_packages = get_conda_packages(prefix);
        let python_package = get_python_package(&conda_packages).unwrap();

        // We expect the following line to panic
        let _ = get_pypi_packages(prefix, &python_package.repodata_record.package_record).unwrap();
    }

    #[test]
    /// Basic test for constructing a [`PythonPackage`] struct
    fn test_python_package() {
        // Fetch a distribution from our test environment first
        let prefix = Path::new(env!("CARGO_MANIFEST_DIR")).join("test-data/test-install-prefix");
        let is_windows = env::consts::OS == "windows";
        let install_paths = InstallPaths::for_venv(PYTHON_VERSION, is_windows);
        let distributions = find_distributions_in_venv(&prefix, &install_paths).unwrap();
        let distribution = distributions.first().unwrap();

        let python_package = PythonPackage::new(&prefix, distribution).unwrap();

        insta::assert_ron_snapshot!(python_package.distribution);
    }
}
