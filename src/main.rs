mod versions;

use anyhow::Context;
use clap::Parser;
use directories::ProjectDirs;
use futures::StreamExt;
use itertools::Itertools;
use serde::Deserialize;
use std::str::FromStr;
use versions::{get_version, get_versions};

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

async fn download_runtime(version: &str, os: Os) -> anyhow::Result<Vec<u8>> {
    let runtime_version = get_version(version).await?;
    Ok(reqwest::get(
        &runtime_version
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
async fn install_runtime(version: &str, os: Os) -> anyhow::Result<()> {
    let data = download_runtime(&version, os).await?;
    println!("data: {:?}", data.len());
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let args = Args::parse();
    let dirs = ProjectDirs::from("com", "Ambient", "AmbientCli")
        .context("Failed to created project dirs")?;

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
            install_runtime(&latest_nightly.version.to_string(), os).await?;
        }
        Commands::Runtime(RuntimeCommands::Install { version }) => {
            install_runtime(&version, os).await?;
        }
        Commands::Variant(args) => {
            println!("args: {:?}", args);
        }
    }

    Ok(())
}
