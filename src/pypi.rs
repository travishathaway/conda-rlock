use std::collections::HashMap;
use std::sync::Arc;

use indexmap::IndexMap;
use rattler_installs_packages::index::{
    ArtifactRequest, CheckAvailablePackages, PackageDb, PackageSourcesBuilder,
};
use rattler_installs_packages::normalize_index_url;
use rattler_installs_packages::python_env::Distribution;
use rattler_installs_packages::resolve::PypiVersion;
use rattler_installs_packages::types::{ArtifactInfo, ArtifactName, NormalizedPackageName};
use reqwest::Client;
use reqwest_middleware::ClientWithMiddleware;
use tokio::runtime::Runtime;
use url::Url;

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
    packages: Vec<Distribution>,
) -> miette::Result<HashMap<NormalizedPackageName, Arc<ArtifactInfo>>> {
    let mut artifacts = HashMap::new();

    let runtime = Runtime::new().unwrap();

    runtime.block_on(async {
        for dist_info in packages {
            let request = ArtifactRequest::FromIndex(dist_info.name.clone());
            let available_artifacts = package_db.available_artifacts(request).await.unwrap();

            let artifact_name = dist_info.name.clone();
            if let Ok(matching_artifact) =
                match_distribution_with_artifact(dist_info.clone(), &available_artifacts)
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
    // if let Some(matching_artifacts) = artifacts.get(&dist_version) {
    //     matching_artifacts.iter().filter_map(|artifact| {
    //         if artifact.python_version.is_none() {
    //             Some(artifact.clone())
    //         } else {
    //             None
    //         }
    //     }).next().ok_or_else(|| miette::miette!("no artifacts found for version: {:?}", distribution.version))
    // } else {
    //     Err(miette::miette!("no artifacts found for version: {:?}", distribution.version))
    // }
}
