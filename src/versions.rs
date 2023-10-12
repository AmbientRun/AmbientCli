use crate::{environment::runtimes_dir, Os, ReleaseTrain};
use anyhow::Context;
use itertools::Itertools;
use serde::Deserialize;
use std::{path::PathBuf, str::FromStr};

#[derive(Debug, Deserialize)]
struct BucketList {
    #[serde(default)]
    items: Vec<BucketItem>,
}
#[derive(Debug, Deserialize)]
struct BucketItem {
    name: String,
    #[serde(rename = "mediaLink")]
    media_link: String,
}

#[derive(Debug, Clone)]
pub struct RuntimeVersion {
    pub version: semver::Version,
    pub builds: Vec<Build>,
}
impl RuntimeVersion {
    pub fn without_builds(version: semver::Version) -> Self {
        Self {
            version,
            builds: Vec::new(),
        }
    }
    pub fn is_nightly(&self) -> bool {
        ReleaseTrain::from_version(&self.version) == ReleaseTrain::Nightly
    }
    pub fn is_point_release(&self) -> bool {
        ReleaseTrain::from_version(&self.version) == ReleaseTrain::Stable
    }
    pub fn is_public(&self) -> bool {
        self.is_point_release() || self.is_nightly()
    }
    pub fn dir_path(&self) -> anyhow::Result<PathBuf> {
        Ok(runtimes_dir()?.join(self.version.to_string()))
    }
    pub fn exe_path(&self) -> anyhow::Result<PathBuf> {
        Ok(self.dir_path()?.join(Os::current().ambient_bin_name()))
    }
    pub fn is_installed(&self) -> anyhow::Result<bool> {
        Ok(self.exe_path()?.exists())
    }
    fn download(&self) -> anyhow::Result<Vec<u8>> {
        let os = Os::current();

        let reponse = ureq::get(
            &self
                .builds
                .iter()
                .find(|b| b.os == os)
                .context("No build for this OS")?
                .url,
        )
        .call()?;

        let mut bytes: Vec<u8> = Vec::new();
        reponse.into_reader().read_to_end(&mut bytes)?;
        Ok(bytes)
    }
    pub fn install(&self) -> anyhow::Result<()> {
        if self.is_installed()? {
            return Ok(());
        }
        println!("Installing runtime version: {}", self.version);
        let data = self.download()?;
        let mut arch = zip::ZipArchive::new(std::io::Cursor::new(data))?;
        let path = runtimes_dir()?.join(self.version.to_string());
        std::fs::create_dir_all(&path)?;
        arch.extract(&path)?;

        println!("Installed at: {:?}", path);
        Ok(())
    }
}
#[derive(Debug, Clone)]
pub struct Build {
    pub os: Os,
    pub url: String,
}

fn version_from_path(path: &str) -> anyhow::Result<semver::Version> {
    let version = path.split('/').nth(1).context("Invalid path")?;
    Ok(semver::Version::parse(version)?)
}

#[derive(Debug, Clone, Copy)]
pub struct VersionsFilter {
    pub include_private: bool,
    pub include_nightly: bool,
}

pub fn get_versions(filter: VersionsFilter) -> anyhow::Result<Vec<RuntimeVersion>> {
    get_versions_with_prefix("", filter)
}
fn get_versions_with_prefix(
    prefix: &str,
    filter: VersionsFilter,
) -> anyhow::Result<Vec<RuntimeVersion>> {
    let builds = ureq::get("https://storage.googleapis.com/storage/v1/b/ambient-artifacts/o")
        .query("prefix", &format!("ambient-builds/{prefix}"))
        .query("alt", "json")
        .call()?
        .into_json::<BucketList>()?;

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
                        os: Os::from_str(build.name.split('/').nth(2).context("Invalid build")?)?,
                        url: build.media_link,
                    })
                })
                .collect::<anyhow::Result<Vec<_>>>()?,
        });
    }
    if !filter.include_private {
        versions.retain(|v| v.is_public());
    }
    if !filter.include_nightly {
        versions.retain(|v| !v.is_nightly());
    }
    versions.sort_by_key(|v| v.version.to_string());
    Ok(versions)
}
pub fn get_version(version: &str) -> anyhow::Result<RuntimeVersion> {
    get_versions_with_prefix(
        version,
        VersionsFilter {
            include_private: true,
            include_nightly: true,
        },
    )?
    .into_iter()
    .next()
    .context("Version not found")
}
