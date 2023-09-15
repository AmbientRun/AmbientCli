mod versions;

use anyhow::Context;
use clap::Parser;
use directories::ProjectDirs;
use futures::StreamExt;
use itertools::Itertools;
use serde::Deserialize;
use std::{path::PathBuf, str::FromStr};
use versions::{get_version, get_versions, RuntimeVersion};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Parser, Clone, Debug)]
pub enum Commands {
    #[command(subcommand)]
    Runtime(RuntimeCommands),
    #[command(external_subcommand)]
    Variant(Vec<String>),
}

#[derive(Parser, Clone, Debug)]
pub enum RuntimeCommands {
    List,
    Install { version: String },
    InstallLatestNightly,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Os {
    Macos,
    Windows,
    Linux,
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

fn app_dir() -> anyhow::Result<ProjectDirs> {
    Ok(ProjectDirs::from("com", "Ambient", "AmbientCli")
        .context("Failed to created project dirs")?)
}
fn runtimes_dir() -> anyhow::Result<PathBuf> {
    Ok(app_dir()?.data_dir().join("runtimes"))
}
async fn download_runtime(version: &RuntimeVersion, os: Os) -> anyhow::Result<Vec<u8>> {
    Ok(reqwest::get(
        &version
            .builds
            .iter()
            .find(|b| b.os == os)
            .context("No build for this OS")?
            .url,
    )
    .await?
    .bytes()
    .await?
    .to_vec())
}
async fn install_runtime(version: &RuntimeVersion, os: Os) -> anyhow::Result<()> {
    println!("Installing runtime version: {}", version.version);
    let data = download_runtime(&version, os).await?;
    let mut arch = zip::ZipArchive::new(std::io::Cursor::new(data))?;
    let path = runtimes_dir()?.join(version.version.to_string());
    arch.extract(&path)?;

    println!("Installed at: {:?}", path);
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let args = Args::parse();

    let os = if cfg!(target_os = "macos") {
        Os::Macos
    } else if cfg!(target_os = "windows") {
        Os::Windows
    } else {
        Os::Linux
    };

    match args.command {
        Commands::Runtime(RuntimeCommands::List) => {
            for build in get_versions().await? {
                println!("{:?}", build);
            }
        }
        Commands::Runtime(RuntimeCommands::InstallLatestNightly) => {
            let latest_nightly = get_versions()
                .await?
                .into_iter()
                .filter(|v| v.is_nightly())
                .last()
                .context("No nightly versions found")?;
            install_runtime(&latest_nightly, os).await?;
        }
        Commands::Runtime(RuntimeCommands::Install { version }) => {
            let runtime_version = get_version(&version).await?;
            install_runtime(&runtime_version, os).await?;
        }
        Commands::Variant(args) => {
            println!("args: {:?}", args);
        }
    }

    Ok(())
}
