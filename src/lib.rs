use std::path::Path;
use std::str::FromStr;
use url::Url;

use log::warn;
use pyo3::prelude::*;
use rattler_conda_types::{ChannelUrl, PackageName, Platform, PrefixRecord};
use rattler_lock::{
    CondaBinaryData, CondaPackageData, LockFile, UrlOrPath,
};
use std::fs::File;
use std::io::Write;

mod pypi;

use crate::pypi::{add_pypi_packages, get_pypi_packages};


// Get all the conda packages for a provided prefix
fn get_conda_packages(prefix: &str) -> Vec<PrefixRecord> {
    let prefix_path = Path::new(prefix);

    PrefixRecord::collect_from_prefix::<PrefixRecord>(prefix_path).unwrap()
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
/// It's important to note that for the time being the only Python package index
/// that is supported is pypi.org. Later, I will make this configurable so that
/// we can point to other indices.
#[pyfunction]
fn lock_prefix(prefix: &str, filename: &str) -> PyResult<()> {
    let conda_packages = get_conda_packages(prefix);
    let environment = "default";
    let mut lock_file = LockFile::builder();

    lock_file.set_channels(
        environment,
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
                environment,
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
        let pypi_packages =
            get_pypi_packages(prefix, &python_package.repodata_record.package_record)
            .map_err(|err| {
                PyErr::new::<pyo3::exceptions::PyOSError, _>(format!("Error retrieving PyPI packages: {:?}", err))
            })?;

        add_pypi_packages(&mut lock_file, environment, Platform::NoArch, pypi_packages)
            .map_err(|err| {
                PyErr::new::<pyo3::exceptions::PyOSError, _>(format!("Error locking PyPI packages: {:?}", err))
            })?;
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
