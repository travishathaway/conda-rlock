//! This module contains all the logic for adding conda packages to a lock file.

use std::path::Path;
use url::Url;

use rattler_conda_types::{ChannelUrl, Platform, PrefixRecord};
use rattler_lock::{CondaBinaryData, CondaPackageData, LockFileBuilder, UrlOrPath};

/// Get all the conda packages for a provided prefix
pub fn get_conda_packages(prefix: &str) -> Vec<PrefixRecord> {
    let prefix_path = Path::new(prefix);

    PrefixRecord::collect_from_prefix::<PrefixRecord>(prefix_path).unwrap() // TODO: handle error
}

/// Used to remove that last to path segments of a URL
fn remove_last_two_segments(mut url: Url) -> Result<Url, Box<dyn std::error::Error>> {
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

/// Add multiple conda packages to a lock file
pub fn add_conda_packages(
    lock_file: &mut LockFileBuilder,
    environment: &str,
    conda_packages: &Vec<PrefixRecord>,
    platform: Platform,
) -> miette::Result<()> {
    for prefix_record in conda_packages {
        let repodata = &prefix_record.repodata_record;
        let package = &prefix_record.repodata_record.package_record;
        let channel_url = get_channel_url_from_package_url(&repodata.url);

        lock_file.add_conda_package(
            environment,
            platform,
            CondaPackageData::Binary(CondaBinaryData {
                package_record: package.clone(),
                location: UrlOrPath::Url(repodata.url.clone()),
                file_name: repodata.file_name.clone(),
                channel: channel_url,
            }),
        );
    }

    Ok(())
}
