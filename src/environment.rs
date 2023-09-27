use crate::ambient_toml::{set_ambient_toml_runtime_version, AmbientToml};
use anyhow::Context;
use directories::ProjectDirs;
use std::{
    path::{Path, PathBuf},
    str::FromStr,
};
use toml_edit::{value, Document, InlineTable};

pub fn app_dir() -> anyhow::Result<ProjectDirs> {
    Ok(ProjectDirs::from("com", "Ambient", "AmbientCli")
        .context("Failed to created project dirs")?)
}
pub fn runtimes_dir() -> anyhow::Result<PathBuf> {
    Ok(app_dir()?.data_dir().join("runtimes"))
}
pub fn settings_dir() -> anyhow::Result<PathBuf> {
    Ok(app_dir()?.config_dir().to_path_buf())
}
pub fn settings_path() -> anyhow::Result<PathBuf> {
    Ok(settings_dir()?.join("settings.json"))
}

pub struct PackagePath(pub PathBuf);
impl PackagePath {
    pub fn from_args_or_local(args: &[String]) -> Self {
        Self::from_args(args).unwrap_or_else(|| Self(std::env::current_dir().unwrap()))
    }
    pub fn from_args(args: &[String]) -> Option<Self> {
        let maybe_path = args.get(1)?;
        if maybe_path.starts_with("--") {
            return None;
        }
        let dir = Path::new(&maybe_path);
        if dir.join("ambient.toml").exists() {
            Some(Self(dir.to_path_buf()))
        } else {
            None
        }
    }
    pub fn ambient_toml(&self) -> AmbientTomlPath {
        AmbientTomlPath(self.0.join("ambient.toml"))
    }
    pub fn cargo_toml(&self) -> CargoTomlPath {
        CargoTomlPath(self.0.join("Cargo.toml"))
    }
    pub fn set_runtime(&self, version: &semver::Version) -> anyhow::Result<()> {
        self.ambient_toml().set_runtime(version)?;
        self.cargo_toml().set_ambient_api(version)?;
        Ok(())
    }
}
pub struct AmbientTomlPath(pub PathBuf);
impl AmbientTomlPath {
    pub fn get_content(&self) -> anyhow::Result<Option<AmbientToml>> {
        if self.0.exists() {
            Ok(Some(AmbientToml::from_file(&self.0)?))
        } else {
            Ok(None)
        }
    }
    pub fn set_runtime(&self, version: &semver::Version) -> anyhow::Result<()> {
        if self.0.exists() {
            set_ambient_toml_runtime_version(&self.0, &format!("{}", version))?;
            println!(
                "Runtime version set to ambient_version=\"{}\" in ambient.toml",
                version
            );
            Ok(())
        } else {
            anyhow::bail!("No ambient.toml found at path {:?}", self.0);
        }
    }
}
pub struct CargoTomlPath(pub PathBuf);
impl CargoTomlPath {
    pub fn set_ambient_api(&self, version: &semver::Version) -> anyhow::Result<()> {
        if self.0.exists() {
            let toml = std::fs::read_to_string(&self.0).context("Failed to read Cargo.toml")?;
            let mut doc = toml.parse::<Document>().context("Invalid Cargo.toml")?;
            set_cargo_toml_ambient_api(&mut doc, version);
            std::fs::write(&self.0, doc.to_string())?;
            println!(
                "Runtime version set to ambient_version=\"{}\" in Cargo.toml",
                version
            );
            Ok(())
        } else {
            anyhow::bail!("No ambient.toml found at path {:?}", self.0);
        }
    }
}
fn set_cargo_toml_ambient_api(doc: &mut toml_edit::Document, version: &semver::Version) {
    let rec = &mut doc["dependencies"]["ambient_api"];
    if version.pre.is_empty() {
        *rec = value(format!("{}", version));
    } else {
        let mut table = InlineTable::default();
        table.insert(
            "git",
            value("https://github.com/AmbientRun/Ambient.git")
                .into_value()
                .unwrap(),
        );
        table.insert("tag", value(format!("v{}", version)).into_value().unwrap());
        *rec = value(table);
    }
}

#[test]
fn test_set_cargo_toml_ambient_api_nightly() {
    let mut doc = r#"
[dependencies]
ambient_api = { git = "https://github.com/AmbientRun/Ambient.git", tag = "v0.3.0-nightly-2023-09-27" }
"#.parse::<Document>().unwrap();
    set_cargo_toml_ambient_api(
        &mut doc,
        &semver::Version::parse("0.3.0-nightly-2023-09-28").unwrap(),
    );
    assert_eq!(
        doc.to_string(),
        r#"
[dependencies]
ambient_api = { git = "https://github.com/AmbientRun/Ambient.git", tag = "v0.3.0-nightly-2023-09-28" }
"#
    );
}

#[test]
fn test_set_cargo_toml_ambient_api_point() {
    let mut doc = r#"
[dependencies]
ambient_api = "0.3.0"
"#
    .parse::<Document>()
    .unwrap();
    set_cargo_toml_ambient_api(&mut doc, &semver::Version::parse("0.4.0").unwrap());
    assert_eq!(
        doc.to_string(),
        r#"
[dependencies]
ambient_api = "0.4.0"
"#
    );
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Os {
    Macos,
    Windows,
    Linux,
}
impl Os {
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Os::Macos
        } else if cfg!(target_os = "windows") {
            Os::Windows
        } else {
            Os::Linux
        }
    }
    pub fn ambient_bin_name(&self) -> &'static str {
        match self {
            Os::Windows => "ambient.exe",
            _ => "ambient",
        }
    }
}
impl std::fmt::Display for Os {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Os::Macos => write!(f, "macos-latest"),
            Os::Windows => write!(f, "windows-latest"),
            Os::Linux => write!(f, "ubuntu-22.04"),
        }
    }
}
impl FromStr for Os {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "macos-latest" => Ok(Os::Macos),
            "windows-latest" => Ok(Os::Windows),
            "ubuntu-22.04" => Ok(Os::Linux),
            _ => Err(anyhow::anyhow!("Invalid OS")),
        }
    }
}
