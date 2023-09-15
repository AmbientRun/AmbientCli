mod environment;
mod versions;

use anyhow::Context;
use clap::Parser;
use directories::ProjectDirs;
use environment::{runtimes_dir, settings_path, Os};
use futures::StreamExt;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
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
    ListInstalled,
    Install { version: String },
    InstallLatestNightly,
    SetDefault { version: String },
}

async fn list_installed_runtimes() -> anyhow::Result<Vec<(semver::Version, PathBuf)>> {
    let runtimes_dir = runtimes_dir()?;
    let mut runtimes = Vec::new();
    for entry in std::fs::read_dir(runtimes_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            let version = semver::Version::parse(entry.file_name().to_str().unwrap())?;
            runtimes.push((version, path.join(Os::current().ambient_bin_name())));
        }
    }
    Ok(runtimes)
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct Settings {
    default_runtime: Option<String>,
}
impl Settings {
    fn load() -> anyhow::Result<Self> {
        Ok(serde_json::from_str(
            std::fs::read_to_string(settings_path()?)?.as_str(),
        )?)
    }
    fn save(&self) -> anyhow::Result<()> {
        std::fs::write(settings_path()?, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let args = Args::parse();

    let mut settings = if settings_path()?.exists() {
        Settings::load()?
    } else {
        Settings::default()
    };

    match args.command {
        Commands::Runtime(RuntimeCommands::List) => {
            for build in get_versions().await? {
                println!("{}", build.version);
            }
        }
        Commands::Runtime(RuntimeCommands::ListInstalled) => {
            for (version, _) in list_installed_runtimes().await? {
                println!("{}", version);
            }
        }
        Commands::Runtime(RuntimeCommands::InstallLatestNightly) => {
            let latest_nightly = get_versions()
                .await?
                .into_iter()
                .filter(|v| v.is_nightly())
                .last()
                .context("No nightly versions found")?;
            latest_nightly.install().await?;
        }
        Commands::Runtime(RuntimeCommands::Install { version }) => {
            let runtime_version = get_version(&version).await?;
            runtime_version.install().await?;
        }
        Commands::Runtime(RuntimeCommands::SetDefault { version }) => {
            let runtime_version = get_version(&version).await?;
            runtime_version.install().await?;
            settings.default_runtime = Some(runtime_version.version.to_string());
            println!(
                "The default runtime version is now {}",
                runtime_version.version.to_string()
            );
        }
        Commands::Variant(args) => {
            println!("args: {:?}", args);
        }
    }

    Ok(())
}
