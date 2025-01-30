use std::path::Path;

use pyo3::prelude::*;
use rattler_conda_types::{PrefixRecord, PackageRecord};

/// Formats the sum of two numbers as string.
#[pyfunction]
fn get_conda_packages(conda_prefix: String) {
    let prefix_path = Path::new(&conda_prefix);

    let prefix_record = PrefixRecord::collect_from_prefix::<PackageRecord>(&prefix_path).unwrap();

    for package in prefix_record {
        println!("{:?}", package);
    }
}

/// A Python module implemented in Rust. The name of this function must match
/// the `lib.name` setting in the `Cargo.toml`, else Python will not be able to
/// import the module.
#[pymodule]
fn conda_rlock(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(get_conda_packages, m)?)?;
    Ok(())
}