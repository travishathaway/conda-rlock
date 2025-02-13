//! This is a library for locking conda environments with rattler lock.
//!
//! It exposes a single function for Python, `lock_prefix`, that accepts a `prefix` and a `filename`.

use std::collections::HashSet;
use std::path::Path;

use pyo3::prelude::*;
use rattler_conda_types::{PackageName, Platform, PrefixRecord};
use rattler_lock::LockFile;
use std::fs::File;
use std::io::Write;

mod conda;
mod pypi;

use crate::conda::{add_conda_packages, get_conda_packages};
use crate::pypi::{add_pypi_packages, get_pypi_packages};

// If Python is listed, return a reference to the package
fn get_python_package(packages: &[PrefixRecord]) -> Option<&PrefixRecord> {
    let name = PackageName::new_unchecked("python");
    packages
        .iter()
        .find(|package| package.repodata_record.package_record.name == name)
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

    // Create a unique iterable of channels
    let channels: HashSet<String> = conda_packages
        .iter()
        .map(|package| {
            package
                .repodata_record
                .channel
                .as_ref()
                .unwrap() // TODO: handle this error
                .to_string()
        })
        .collect();

    lock_file.set_channels(environment, channels);

    add_conda_packages(&mut lock_file, environment, &conda_packages).map_err(|err| {
        PyErr::new::<pyo3::exceptions::PyOSError, _>(format!(
            "Error locking conda packages: {:?}",
            err
        ))
    })?;

    if let Some(python_package) = get_python_package(&conda_packages) {
        let pypi_packages =
            get_pypi_packages(prefix, &python_package.repodata_record.package_record).map_err(
                |err| {
                    PyErr::new::<pyo3::exceptions::PyOSError, _>(format!(
                        "Error retrieving PyPI packages: {:?}",
                        err
                    ))
                },
            )?;

        // TODO: The platform is not correct. We need to inspect each individual Python package
        //       to get the correct value.
        add_pypi_packages(&mut lock_file, environment, Platform::NoArch, pypi_packages).map_err(
            |err| {
                PyErr::new::<pyo3::exceptions::PyOSError, _>(format!(
                    "Error locking PyPI packages: {:?}",
                    err
                ))
            },
        )?;
    }

    let lockfile_str = lock_file.finish().render_to_string().unwrap(); // TODO: Handle error

    write_string_to_file(filename, &lockfile_str).map_err(|err| {
        PyErr::new::<pyo3::exceptions::PyOSError, _>(format!("Error writing lockfile: {:?}", err))
    })?;

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
