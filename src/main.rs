mod ambient_toml;
mod environment;
mod versions;

use ambient_toml::AmbientToml;
use anyhow::Context;
use clap::Parser;
use colored::Colorize;
use directories::ProjectDirs;
use environment::{runtimes_dir, settings_path, Os};
use futures::StreamExt;
use itertools::Itertools;
use semver::VersionReq;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, str::FromStr};
use versions::{get_version, get_versions, RuntimeVersion, VersionsFilter};

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
}

#[derive(Parser, Clone, Debug)]
pub enum RuntimeCommands {
    List,
    ListInstalled,
    Install {
        version: String,
    },
    SetDefault {
        version: String,
    },
    /// Show the runtime version that will be used by default in this directory
    Current,
    /// Show where the settings file is located
    ShowSettingsPath,
    /// Remove all installed runtime versions
    UninstallAll,
}

async fn list_installed_runtimes() -> anyhow::Result<Vec<(semver::Version, PathBuf)>> {
    let runtimes_dir = runtimes_dir()?;
    if !runtimes_dir.exists() {
        return Ok(Vec::new());
    }
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
    default_runtime: Option<semver::Version>,
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

async fn get_version_satisfying_req(
    settings: &Settings,
    version_req: &VersionReq,
) -> anyhow::Result<RuntimeVersion> {
    log::info!("Looking for version satisfying {}", version_req);
    if let Some(default_version) = &settings.default_runtime {
        log::info!("Checking default version: {}", default_version);
        if version_req.matches(default_version) {
            log::info!("Default version matches, returning.");
            return Ok(RuntimeVersion::without_builds(default_version.clone()));
        }
    }
    log::info!("Checking installed versions");
    for (version, _) in list_installed_runtimes().await? {
        if version_req.matches(&version) {
            return Ok(RuntimeVersion::without_builds(version));
        }
    }
    log::info!("Checking all versions");
    for version in get_versions(VersionsFilter {
        include_private: true,
        include_nightly: true,
    })
    .await?
    {
        if version_req.matches(&version.version) {
            return Ok(version);
        }
    }
    anyhow::bail!("No version found satisfying {}", version_req);
}

async fn get_latest_remote_version() -> anyhow::Result<RuntimeVersion> {
    let versions = get_versions(VersionsFilter {
        include_private: false,
        include_nightly: true,
    })
    .await?;
    if let Some(v) = versions.iter().filter(|v| v.is_point_release()).last() {
        return Ok(v.clone());
    } else {
        Ok(versions.last().cloned().context("No versions found")?)
    }
}

async fn get_current_runtime(settings: &Settings) -> anyhow::Result<RuntimeVersion> {
    if AmbientToml::exists() {
        let toml = AmbientToml::current()?;
        if let Some(version_req) = toml.package.ambient_version {
            return get_version_satisfying_req(settings, &version_req).await;
        }
    }
    match &settings.default_runtime {
        Some(version) => Ok(RuntimeVersion::without_builds(version.clone())),
        None => {
            log::info!("No default runtime version set, using latest remote version");
            let version = get_latest_remote_version().await?;
            log::info!("Found latest remote version: {}", version.version);
            Ok(version)
        }
    }
}

async fn version_manager_main(mut settings: Settings) -> anyhow::Result<()> {
    let args = Args::parse();

    match args.command {
        Commands::Runtime(RuntimeCommands::List) => {
            for build in get_versions(VersionsFilter {
                include_private: true,
                include_nightly: true,
            })
            .await?
            {
                println!("{}", build.version);
            }
        }
        Commands::Runtime(RuntimeCommands::ListInstalled) => {
            for (version, _) in list_installed_runtimes().await? {
                println!("{}", version);
            }
        }
        Commands::Runtime(RuntimeCommands::Install { version }) => {
            let runtime_version = get_version(&version).await?;
            runtime_version.install().await?;
        }
        Commands::Runtime(RuntimeCommands::SetDefault { version }) => {
            let runtime_version = get_version(&version).await?;
            runtime_version.install().await?;
            settings.default_runtime = Some(runtime_version.version.clone());
            settings.save()?;
            println!(
                "The default runtime version is now {}",
                runtime_version.version.to_string()
            );
        }
        Commands::Runtime(RuntimeCommands::Current) => {
            let version = get_current_runtime(&settings).await?;
            println!("{}", version.version);
        }
        Commands::Runtime(RuntimeCommands::ShowSettingsPath) => {
            println!("{}", settings_path()?.to_string_lossy());
        }
        Commands::Runtime(RuntimeCommands::UninstallAll) => {
            std::fs::remove_dir_all(runtimes_dir()?)?;
            std::fs::create_dir_all(runtimes_dir()?)?;
        }
    }
    Ok(())
}

async fn runtime_exec(settings: &Settings, args: Vec<String>) -> anyhow::Result<()> {
    let version = get_current_runtime(settings).await?;
    version.install().await?;
    let mut process = std::process::Command::new(version.exe_path()?)
        .args(args)
        .spawn()?;
    process.wait()?;
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let settings = if settings_path()?.exists() {
        Settings::load()?
    } else {
        Settings::default()
    };

    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.get(0) == Some(&"runtime".to_string()) {
        version_manager_main(settings).await?;
    } else if args.get(0) == Some(&"--help".to_string()) {
        runtime_exec(&settings, args).await?;
        println!("");
        println!(
            "{}",
            "Runtime version manager commands:"
                .white()
                .bold()
                .underline()
        );
        println!(
            "  {} Install and manage runtime versions",
            "runtime".white().bold()
        );
    } else {
        runtime_exec(&settings, args).await?;
    }

    Ok(())
}
