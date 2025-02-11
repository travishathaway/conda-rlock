use rattler_installs_packages::artifacts;
use rattler_installs_packages::types::NormalizedPackageName;
use std::collections::HashMap;
use std::env;
use std::path::Path;
use std::str::FromStr;
use url::Url;

use log::warn;
use pep440_rs::{Version as Pep440Version, VersionSpecifiers as Pep440VersionSpecifiers};
use pep508_rs::PackageName as Pep508PackageName;
use pyo3::prelude::*;
use rattler_conda_types::{
    ChannelUrl, Component, PackageName, PackageRecord, Platform, PrefixRecord, Version,
};
use rattler_installs_packages::install::InstallPaths;
use rattler_installs_packages::python_env::{find_distributions_in_venv, Distribution, WheelTag};
use rattler_lock::{
    CondaBinaryData, CondaPackageData, LockFile, PackageHashes, PypiPackageData, UrlOrPath,
};
use std::fs::File;
use std::io::Write;

mod pypi;

use crate::pypi::{get_available_artifacts, get_package_db};

// Get all the conda packages for a provided prefix
fn get_conda_packages(prefix: &str) -> Vec<PrefixRecord> {
    let prefix_path = Path::new(prefix);

    PrefixRecord::collect_from_prefix::<PrefixRecord>(prefix_path).unwrap()
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

// Get all the PyPI packages for a provided prefix
fn get_pypi_packages(prefix: &str, python_package: &PackageRecord) -> Vec<Distribution> {
    let prefix_path = Path::new(prefix);
    let is_windows = env::consts::OS == "windows";
    let version_components = get_major_minor_bug(&python_package.version).unwrap();
    let version_components = (
        version_components.0 as u32,
        version_components.1 as u32,
        version_components.2 as u32,
    );
    let install_paths = InstallPaths::for_venv(version_components, is_windows);

    find_distributions_in_venv(prefix_path, &install_paths)
        .unwrap()
        .iter()
        .filter(|dist| dist.installer.as_ref().unwrap_or(&String::new()) == "pip")
        .cloned()
        .collect()
}

// If Python is listed, return a reference to the package
fn get_python_package(packages: &[PrefixRecord]) -> Option<&PrefixRecord> {
    let name = PackageName::new_unchecked("python");
    packages
        .iter()
        .find(|package| package.repodata_record.package_record.name == name)
}

pub fn remove_last_two_segments(mut url: Url) -> Result<Url, Box<dyn std::error::Error>> {
    let mut segments: Vec<&str> = url.path_segments().ok_or("cannot be base")?.collect();

    if segments.len() >= 2 {
        segments.pop();
        segments.pop();
    } else {
        return Err("URL does not have enough segments".into());
    }

    let new_path = segments.join("/");
    url.set_path(&new_path);

    Ok(url)
}

/// Provided a URL to a package return th channel URL
fn get_channel_url_from_package_url(url: &Url) -> Option<ChannelUrl> {
    if let Ok(base_url) = remove_last_two_segments(url.clone()) {
        Some(ChannelUrl::from(base_url))
    } else {
        // TODO: might want to log this case
        None
    }
}

fn write_string_to_file(filename: &str, content: &str) -> std::io::Result<()> {
    let path = Path::new(filename);
    let mut file = File::create(path)?;
    file.write_all(content.as_bytes())?;
    Ok(())
}

/// Locks a prefix to a lockfile
///
/// For this function, we need to do the following:
///
/// - Create a LockFileBuilder
/// - Look at the tests in this file:
///   https://github.com/conda/rattler/blob/main/crates/rattler_lock/src/builder.rs
///
///   I'll need to basically copy what's going on in there.
///
///
#[pyfunction]
fn lock_prefix(prefix: &str, filename: &str) -> PyResult<()> {
    let conda_packages = get_conda_packages(prefix);
    let mut lock_file = LockFile::builder();

    lock_file.set_channels(
        "default",
        conda_packages.iter().map(|package| {
            package
                .repodata_record
                .channel
                .as_ref()
                .unwrap()
                .to_string()
        }),
    );

    // TODO: refactor this to its own function
    for prefix_record in &conda_packages {
        let channel_url = get_channel_url_from_package_url(&prefix_record.repodata_record.url);

        if let Ok(platform) =
            Platform::from_str(&prefix_record.repodata_record.package_record.subdir)
        {
            lock_file.add_conda_package(
                "default",
                platform,
                CondaPackageData::Binary(CondaBinaryData {
                    package_record: prefix_record.repodata_record.package_record.clone(),
                    location: UrlOrPath::Url(prefix_record.repodata_record.url.clone()),
                    file_name: prefix_record.repodata_record.file_name.clone(),
                    channel: channel_url,
                }),
            );
        } else {
            warn!(
                "Could not find platform for package: {:?}",
                prefix_record.repodata_record.package_record
            );
        }
    }

    if let Some(python_package) = get_python_package(&conda_packages) {
        let index_url = "https://pypi.org/simple";
        let package_db = get_package_db(index_url).unwrap(); // TODO: handle error
        let pypi_packages =
            get_pypi_packages(prefix, &python_package.repodata_record.package_record);

        let pypi_artifacts = get_available_artifacts(&package_db, &pypi_packages).unwrap();

        let mut distribution_lookup: HashMap<NormalizedPackageName, Distribution> = HashMap::new();

        for dist in pypi_packages {
            distribution_lookup.insert(dist.name, dist);
        }

        // TODO: We got some problems...
        //       I have no idea about the best way to get the platform for the PyPI packages
        //       I also need to access both the `Distribution` struct and the `ArtifactInfo`
        //       struct at the same time. I need to rework some stuff to do this in a sane manner.
        //       Maybe create a new struct that combines the two structs?

        for (name, artifact) in pypi_artifacts {
            let dist = distribution_lookup.get(&name).unwrap();

            println!("{:?}", dist.tags);

            lock_file.add_pypi_package(
                "default",
                Platform::NoArch,
                PypiPackageData {
                    name: Pep508PackageName::new(name.to_string()).unwrap(), // TODO: handle error
                    // TODO: The following is not nice; I do it because rattler_installs_packages and rattler_lock
                    //       depend on different versions of pep440_rs and pep508_rs.
                    version: Pep440Version::from_str(&dist.version.to_string()).unwrap(),
                    location: UrlOrPath::Url(artifact.url.clone()),
                    hash: Some(
                        PackageHashes::Sha256(artifact.hashes.unwrap().sha256.unwrap()), // TODO: handle unwraps better
                    ),
                    requires_dist: artifact,
                    requires_python: Some(
                        Pep440VersionSpecifiers::from_str(
                            &artifact.requires_python.unwrap().to_string(),
                        )
                        .unwrap(),
                    ),
                    editable: false,
                },
            );
        }

        for package in get_pypi_packages(prefix, &python_package.repodata_record.package_record) {
            println!("{}", package.dist_info.display());
            //lock_file.add_pypi_package(
            //    "default",
            //    platform,
            //    PypiPackageData {
            //        name: package.name,
            //        version: package.version.clone(),
            //        location: todo!(),
            //        hash: todo!(),
            //        requires_dist: todo!(),
            //        requires_python: todo!(),
            //        editable: todo!()
            //    },
            //

            //);
        }
    }

    let lockfile_str = lock_file.finish().render_to_string().unwrap();
    write_string_to_file(filename, &lockfile_str)?;

    Ok(())
}

/// A Python module implemented in Rust. The name of this function must match
/// the `lib.name` setting in the `Cargo.toml`, else Python will not be able to
/// import the module.
#[pymodule]
fn conda_rlock(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(lock_prefix, m)?)?;
    Ok(())
}
