use semver::VersionReq;
use serde::Deserialize;
use std::path::Path;

// This is a subset of the actual ambient.toml, so that it will be compatible with as many different versions as possible.
#[derive(Debug, Deserialize)]
pub struct AmbientToml {
    pub package: Package,
}
impl AmbientToml {
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
