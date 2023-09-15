use crate::Os;
use anyhow::Context;
use itertools::Itertools;
use serde::Deserialize;
use std::str::FromStr;

#[derive(Debug, Deserialize)]
struct BucketList {
    items: Vec<BucketItem>,
}
#[derive(Debug, Deserialize)]
struct BucketItem {
    name: String,
    mediaLink: String,
}

#[derive(Debug)]
pub struct RuntimeVersion {
    pub version: semver::Version,
    pub builds: Vec<Build>,
}
impl RuntimeVersion {
    pub fn is_nightly(&self) -> bool {
        self.version.to_string().contains("nightly")
    }
}
#[derive(Debug)]
pub struct Build {
    pub os: Os,
    pub url: String,
}

fn version_from_path(path: &str) -> anyhow::Result<semver::Version> {
    let version = path.split("/").nth(1).context("Invalid path")?;
    Ok(semver::Version::parse(version)?)
}

pub async fn get_versions() -> anyhow::Result<Vec<RuntimeVersion>> {
    get_versions_with_prefix("").await
}
async fn get_versions_with_prefix(prefix: &str) -> anyhow::Result<Vec<RuntimeVersion>> {
    let client = reqwest::Client::new();

    let builds = client
        .get("https://storage.googleapis.com/storage/v1/b/ambient-artifacts/o")
        .query(&[
            ("prefix", &format!("ambient-builds/{prefix}") as &str),
            ("alt", "json"),
        ])
        .send()
        .await?
        .json::<BucketList>()
        .await?;
    let builds = builds
        .items
        .into_iter()
        .filter_map(|b| Some((version_from_path(&b.name).ok()?, b)))
        .collect_vec();
    let mut versions = Vec::new();
    for (version, builds) in builds.into_iter().group_by(|x| x.0.clone()).into_iter() {
        versions.push(RuntimeVersion {
            version,
            builds: builds
                .map(|(_, build)| {
                    Ok(Build {
                        os: Os::from_str(build.name.split("/").nth(2).context("Invalid build")?)?,
                        url: build.mediaLink,
                    })
                })
                .collect::<anyhow::Result<Vec<_>>>()?,
        });
    }
    versions.sort_by_key(|v| v.version.to_string());
    Ok(versions)
}
pub async fn get_version(version: &str) -> anyhow::Result<RuntimeVersion> {
    Ok(get_versions_with_prefix(version)
        .await?
        .into_iter()
        .next()
        .context("Version not found")?)
}
