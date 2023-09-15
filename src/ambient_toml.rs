use semver::VersionReq;
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct AmbientToml {
    pub package: Package,
}
impl AmbientToml {
    pub fn exists() -> bool {
        Path::new("ambient.toml").exists()
    }
    pub fn current() -> anyhow::Result<Self> {
        Self::from_file("ambient.toml")
    }
    pub fn from_file(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        Ok(toml::from_str(
            std::fs::read_to_string(path.as_ref())?.as_str(),
        )?)
    }
}

#[derive(Debug, Deserialize)]
pub struct Package {
    pub ambient_version: Option<VersionReq>,
}
