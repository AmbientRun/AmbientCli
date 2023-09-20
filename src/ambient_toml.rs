use anyhow::Context;
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

pub fn set_ambient_toml_runtime_version(
    path: impl AsRef<Path>,
    version: &str,
) -> anyhow::Result<()> {
    use toml_edit::{value, Document};
    let toml = std::fs::read_to_string(&path).context("Failed to read ambient.toml")?;
    let mut doc = toml.parse::<Document>().context("Invalid ambient.toml")?;
    doc["package"]["ambient_version"] = value(version);
    std::fs::write(path, doc.to_string())?;
    Ok(())
}
