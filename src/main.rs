mod ambient_toml;
mod environment;
mod versions;

use ambient_toml::AmbientToml;
use clap::Parser;
use colored::Colorize;
use environment::{runtimes_dir, settings_dir, settings_path, Os};
use semver::VersionReq;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
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
    /// List all available runtime versions
    ListAll,
    /// List locally installed runtime versions
    ListInstalled,
    /// Install a specific runtime version
    Install { version: String },
    /// Update the default runtime version to the latest available
    UpdateDefault,
    /// Set the global default version
    SetDefault { version: String },
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

#[derive(Debug, Clone, Copy, PartialEq)]
enum ReleaseTrain {
    Stable,
    Nightly,
    Internal,
}
impl ReleaseTrain {
    pub fn from_version(version: &semver::Version) -> Self {
        if version.pre.is_empty() {
            ReleaseTrain::Stable
        } else {
            if version.pre.contains("nightly") {
                ReleaseTrain::Nightly
            } else {
                ReleaseTrain::Internal
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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
        std::fs::create_dir_all(settings_dir()?)?;
        std::fs::write(settings_path()?, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
    fn release_train(&self) -> ReleaseTrain {
        self.default_runtime
            .as_ref()
            .map(|v| ReleaseTrain::from_version(v))
            .unwrap_or(ReleaseTrain::Stable)
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

async fn get_latest_remote_version_for_train(
    release_train: ReleaseTrain,
    fallback_to_nightly: bool,
) -> anyhow::Result<RuntimeVersion> {
    let versions = get_versions(VersionsFilter {
        include_private: release_train == ReleaseTrain::Internal,
        include_nightly: release_train == ReleaseTrain::Nightly || fallback_to_nightly,
    })
    .await?;
    let latest_for_train = versions
        .iter()
        .filter(|v| release_train == ReleaseTrain::from_version(&v.version))
        .last()
        .cloned();
    if let Some(latest_for_train) = latest_for_train {
        return Ok(latest_for_train);
    } else if fallback_to_nightly {
        let latest_nightly = versions.iter().filter(|v| v.is_nightly()).last().cloned();
        if let Some(latest_nightly) = latest_nightly {
            return Ok(latest_nightly);
        }
    }
    Err(anyhow::anyhow!("No versions found for {:?}", release_train))
}

async fn get_current_runtime(
    settings: &Settings,
    ambient_toml: &Option<AmbientToml>,
) -> anyhow::Result<RuntimeVersion> {
    if let Some(toml) = ambient_toml {
        if let Some(version_req) = &toml.package.ambient_version {
            return get_version_satisfying_req(settings, version_req).await;
        }
    }
    match &settings.default_runtime {
        Some(version) => Ok(RuntimeVersion::without_builds(version.clone())),
        None => {
            anyhow::bail!("No default runtime version set")
        }
    }
}

async fn set_default_runtime(
    settings: &mut Settings,
    version: &RuntimeVersion,
) -> anyhow::Result<()> {
    version.install().await?;
    settings.default_runtime = Some(version.version.clone());
    settings.save()?;
    println!(
        "The default runtime version is now {}",
        version.version.to_string()
    );
    Ok(())
}

async fn version_manager_main(mut settings: Settings) -> anyhow::Result<()> {
    let args = Args::parse();

    match args.command {
        Commands::Runtime(RuntimeCommands::ListAll) => {
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
            set_default_runtime(&mut settings, &runtime_version).await?;
        }
        Commands::Runtime(RuntimeCommands::UpdateDefault) => {
            let version =
                get_latest_remote_version_for_train(settings.release_train(), false).await?;
            set_default_runtime(&mut settings, &version).await?;
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

async fn runtime_exec(mut settings: Settings, args: Vec<String>) -> anyhow::Result<()> {
    if settings.default_runtime.is_none() {
        println!("No default runtime version set, installing latest stable version");
        let version = get_latest_remote_version_for_train(ReleaseTrain::Stable, true).await?;
        set_default_runtime(&mut settings, &version).await?;
    }
    let ambient_toml = get_ambient_toml(&args)?;
    let version = get_current_runtime(&settings, &ambient_toml).await?;
    version.install().await?;
    let mut process = std::process::Command::new(version.exe_path()?)
        .args(args)
        .spawn()?;
    process.wait()?;
    Ok(())
}

fn project_dir_from_args(args: &[String]) -> Option<PathBuf> {
    let maybe_path = args.get(1)?;
    if maybe_path.starts_with("--") {
        return None;
    }
    let dir = Path::new(&maybe_path);
    if dir.join("ambient.toml").exists() {
        Some(dir.to_path_buf())
    } else {
        None
    }
}
fn ambient_toml_path_from_args(args: &[String]) -> PathBuf {
    match project_dir_from_args(args) {
        Some(path) => path.join("ambient.toml"),
        None => Path::new("ambient.toml").to_path_buf(),
    }
}

fn get_ambient_toml(args: &[String]) -> anyhow::Result<Option<AmbientToml>> {
    let path = ambient_toml_path_from_args(&args);
    if path.exists() {
        Ok(Some(AmbientToml::from_file(path)?))
    } else {
        Ok(None)
    }
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
        runtime_exec(settings, args).await?;
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
        runtime_exec(settings, args).await?;
    }

    Ok(())
}
