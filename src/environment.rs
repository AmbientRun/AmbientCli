use crate::ambient_toml::{set_ambient_toml_runtime_version, AmbientToml};
use anyhow::Context;
use directories::ProjectDirs;
use std::{
    path::{Path, PathBuf},
    str::FromStr,
};

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
    pub fn set_runtime(&self, version: &str) -> anyhow::Result<()> {
        if self.0.exists() {
            set_ambient_toml_runtime_version(&self.0, version)?;
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
