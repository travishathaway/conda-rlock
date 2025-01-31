use std::env;
use std::path::Path;

use pyo3::prelude::*;
use rattler_conda_types::{Component, PackageName, PackageRecord, PrefixRecord, Version};
use rattler_installs_packages::install::InstallPaths;
use rattler_installs_packages::python_env::{find_distributions_in_venv, Distribution};

// Get all the conda packages for a provided prefix
fn get_conda_packages(prefix: &str) -> Vec<PackageRecord> {
    let prefix_path = Path::new(prefix);

    PrefixRecord::collect_from_prefix::<PackageRecord>(prefix_path).unwrap()
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

    find_distributions_in_venv(prefix_path, &install_paths).unwrap()
}

// If Python is listed, return a reference to the package
fn get_python_package(packages: &[PackageRecord]) -> Option<&PackageRecord> {
    let name = PackageName::new_unchecked("python");
    packages.iter().find(|package| package.name == name)
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
fn lock_prefix(prefix: &str) -> PyResult<()> {
    let conda_packages = get_conda_packages(prefix);

    println!("Conda packages for prefix: {}", prefix);
    for package in &conda_packages {
        println!("- {}", package.name.as_normalized());
    }

    if let Some(python_package) = get_python_package(&conda_packages) {
        println!("PyPI packages for prefix: {}", prefix);
        // println!("Python version: {}", python_package.version);
        for package in get_pypi_packages(prefix, python_package) {
            println!("- {}", package.name);
        }
    }
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
