use std::env;
use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;

use indexmap::IndexMap;
/// TODO: This is necessary because of the version mismatch between rattler_installs_packages and rattler_lock
///       I'm hoping this will be fixed once rattler_pypi_interop is available as a crate in rattler proper.
use pep440_rs::{Version as Pep440Version, VersionSpecifiers as Pep440VersionSpecifiers};
use pep508_rs::{PackageName as Pep508PackageName, Requirement as Pep508Requirement};
use rattler_conda_types::{Component, Platform, PackageRecord, Version};
use rattler_lock::{LockFileBuilder, PackageHashes, PypiPackageData, PypiPackageEnvironmentData, UrlOrPath};
use rattler_installs_packages::index::{
    ArtifactRequest, CheckAvailablePackages, PackageDb, PackageSourcesBuilder,
};
use rattler_installs_packages::install::InstallPaths;
use rattler_installs_packages::normalize_index_url;
use rattler_installs_packages::python_env::{find_distributions_in_venv, Distribution};
use rattler_installs_packages::resolve::PypiVersion;
use rattler_installs_packages::types::{ArtifactInfo, ArtifactName, NormalizedPackageName, WheelCoreMetadata};
use reqwest::Client;
use reqwest_middleware::ClientWithMiddleware;
use tokio::runtime::Runtime;
use url::Url;

#[derive(Debug, Clone)]
/// Used to store both information about the distribution and the metadata of the wheel
pub struct PythonPackage{
    /// The distribution information
    distribution: Distribution,
    /// The metadata of the wheel found in the `METADATA` file of the Python package
    metadata: WheelCoreMetadata
}

/// Get the major, minor, and bug version components of a version
///
/// Adding this here because ``ratter_conda_types::Version`` doesn't have a method
/// to get the major, minor, and bug version components.
fn get_major_minor_bug(version: &Version) -> Option<(u64, u64, u64)> {
    let mut segments = version.segments();
    let major_segment = segments.next()?;
    let minor_segment = segments.next()?;
    let bug_segment = segments.next()?;

    if major_segment.component_count() == 1
        && minor_segment.component_count() == 1
        && bug_segment.component_count() == 1
    {
        Some((
            major_segment
                .components()
                .next()
                .and_then(Component::as_number)?,
            minor_segment
                .components()
                .next()
                .and_then(Component::as_number)?,
            bug_segment
                .components()
                .next()
                .and_then(Component::as_number)?,
        ))
    } else {
        None
    }
}

/// Get all the PyPI packages for a provided prefix
pub fn get_pypi_packages(prefix: &str, python_package: &PackageRecord) -> miette::Result<Vec<PythonPackage>> {
    let prefix_path = Path::new(prefix);
    let is_windows = env::consts::OS == "windows";
    let version_components = get_major_minor_bug(&python_package.version).unwrap();
    let version_components = (
        version_components.0 as u32,
        version_components.1 as u32,
        version_components.2 as u32,
    );
    let install_paths = InstallPaths::for_venv(version_components, is_windows);

    let distributions = find_distributions_in_venv(prefix_path, &install_paths)
        .map_err(|err| miette::miette!("failed to find distributions in venv: {:?}", err))?;

    distributions
        .iter()
        .filter(|dist| dist.installer.as_ref().unwrap_or(&String::new()) == "pip")
        .map(|dist| {
            get_python_package(prefix_path, dist)
        })
        .collect()
}

/// Using both the `prefix` and `Distribution.dist_info` fields, this function reads the metadata
/// from the `METADATA` file that should be on disk. We then return a `PythonPackage` struct that
/// provides both information about the distribution and its metadata in a single struct.
/// 
/// TODO: Should this actually be a method on the `PythonPackage` struct itself?
pub fn get_python_package(prefix: &Path, distribution: &Distribution) -> miette::Result<PythonPackage> {
    let path_to_metadata = prefix.join(&distribution.dist_info).join("METADATA");

    let metadata_bytes = std::fs::read(&path_to_metadata)
        .map_err(|err| miette::miette!("failed to read metadata file: {:?}", err))?;

    let metadata = WheelCoreMetadata::try_from(metadata_bytes.as_slice())
        .map_err(|err| miette::miette!("failed to parse metadata: {:?}", err))?;

    let package = PythonPackage {
        distribution: distribution.clone(),
        metadata,
    };

    Ok(package)
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

    let runtime = Runtime::new().unwrap();

    runtime.block_on(async {
        for package in packages {
            let request = ArtifactRequest::FromIndex(package.distribution.name.clone());
            let available_artifacts = package_db.available_artifacts(request).await.unwrap();

            let artifact_name = package.distribution.name.clone();
            if let Ok(matching_artifact) =
                match_distribution_with_artifact(package.distribution.clone(), &available_artifacts)
            {
                artifacts.insert(artifact_name, matching_artifact);
            }
        }
    });

    Ok(artifacts)
}

/// A function that matches information provided as a `Distribution` struct with the data returned
/// from the `PackageDb::available_artifacts` method.
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
pub fn add_pypi_packages(
    lock_file: &mut LockFileBuilder,
    environment: &str,
    platform: Platform,
    packages: Vec<PythonPackage>
) -> miette::Result<()> {
    let index_url = "https://pypi.org/simple";
    let package_db = get_package_db(index_url).unwrap(); // TODO: handle error

    let artifacts = get_available_artifacts(&package_db, &packages).unwrap();

    let mut distribution_lookup: HashMap<NormalizedPackageName, PythonPackage> = HashMap::new();

    for package in packages {
        distribution_lookup.insert(package.distribution.name.clone(), package);
    }

    for (name, artifact) in artifacts {
        let package = distribution_lookup.get(&name).unwrap();

        lock_file.add_pypi_package(
            environment,
            platform,
            PypiPackageData {
                name: Pep508PackageName::new(name.to_string()).map_err(|e| miette::miette!("Failed to create package name: {:?}", e))?,
                version: Pep440Version::from_str(&package.distribution.version.to_string()).map_err(|e| miette::miette!("Failed to parse version: {:?}", e))?,
                location: UrlOrPath::Url(artifact.url.clone()),
                hash: artifact.hashes.as_ref()
                    .and_then(|hashes| hashes.sha256.clone())
                    .map(PackageHashes::Sha256)
                ,
                requires_dist: package.metadata.requires_dist
                    .iter()
                    .map(|req| Pep508Requirement::from_str(&req.to_string()).map_err(|e| miette::miette!("Failed to parse requirement: {:?}", e)))
                    .collect::<Result<Vec<_>, _>>()?,
                requires_python: artifact.requires_python.as_ref()
                    .map(|req| Pep440VersionSpecifiers::from_str(&req.to_string()).map_err(|e| miette::miette!("Failed to parse requires_python: {:?}", e)))
                    .transpose()?,
                editable: false,
            },
            PypiPackageEnvironmentData::default(),
        );
    }

    Ok(())
}
