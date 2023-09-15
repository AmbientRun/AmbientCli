use anyhow::Context;
use directories::ProjectDirs;
use std::{path::PathBuf, str::FromStr};

pub fn app_dir() -> anyhow::Result<ProjectDirs> {
    Ok(ProjectDirs::from("com", "Ambient", "AmbientCli")
        .context("Failed to created project dirs")?)
}
pub fn runtimes_dir() -> anyhow::Result<PathBuf> {
    Ok(app_dir()?.data_dir().join("runtimes"))
}
pub fn settings_path() -> anyhow::Result<PathBuf> {
    Ok(app_dir()?.config_dir().join("settings.json"))
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
