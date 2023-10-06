mod ambient_toml;
mod environment;
mod versions;

use anyhow::Context;
use clap::Parser;
use colored::Colorize;
use environment::{runtimes_dir, settings_dir, settings_path, Os, PackagePath};
use semver::VersionReq;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
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
    /// Update the runtime version for the local package
    UpdateLocal,
    /// Set the global default version
    SetDefault { version: String },
    /// Set the local package ambient runtime version
    SetLocal { version: String },
    /// Show where the settings file is located
    ShowSettingsPath,
    /// Remove all installed runtime versions
    UninstallAll,
}

fn list_installed_runtimes() -> anyhow::Result<Vec<(semver::Version, PathBuf)>> {
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
        } else if version.pre.contains("nightly") {
            ReleaseTrain::Nightly
        } else {
            ReleaseTrain::Internal
        }
    }
    pub fn from_version_req(version_req: &semver::VersionReq) -> Self {
        for comp in &version_req.comparators {
            if comp.pre.is_empty() {
                return ReleaseTrain::Stable;
            } else if comp.pre.contains("nightly") {
                return ReleaseTrain::Nightly;
            } else {
                return ReleaseTrain::Internal;
            }
        }
        ReleaseTrain::Stable
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

/// If the version requirement contains a pre-release identifier, only versions with the same pre-release identifier will be considered.
fn matches_exact(version_req: &VersionReq, version: &semver::Version) -> bool {
    for comp in &version_req.comparators {
        if !comp.pre.is_empty() || !version.pre.is_empty() {
            return comp.matches(version) && comp.pre == version.pre;
        }
    }
    version_req.matches(version)
}

fn get_version_satisfying_req(
    settings: &Settings,
    version_req: &VersionReq,
) -> anyhow::Result<RuntimeVersion> {
    log::info!("Looking for version satisfying {}", version_req);
    if let Some(default_version) = &settings.default_runtime {
        log::info!("Checking default version: {}", default_version);
        if matches_exact(version_req, default_version) {
            log::info!("Default version matches, returning.");
            return Ok(RuntimeVersion::without_builds(default_version.clone()));
        }
    }
    log::info!("Checking installed versions");
    for (version, _) in list_installed_runtimes()? {
        if matches_exact(version_req, &version) {
            return Ok(RuntimeVersion::without_builds(version));
        }
    }
    log::info!("Checking all versions");
    for version in get_versions(VersionsFilter {
        include_private: true,
        include_nightly: true,
    })? {
        if matches_exact(version_req, &version.version) {
            return Ok(version);
        }
    }
    anyhow::bail!("No version found satisfying {}", version_req);
}

fn get_latest_remote_version_for_train(
    release_train: ReleaseTrain,
    fallback_to_nightly: bool,
) -> anyhow::Result<RuntimeVersion> {
    let versions = get_versions(VersionsFilter {
        include_private: release_train == ReleaseTrain::Internal,
        include_nightly: release_train == ReleaseTrain::Nightly || fallback_to_nightly,
    })?;
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

fn get_current_runtime(
    settings: &Settings,
    package_path: &Option<PackagePath>,
) -> anyhow::Result<RuntimeVersion> {
    if let Some(package_path) = package_path {
        let ambient_toml = package_path
            .ambient_toml()
            .get_content()?
            .context("No ambient.toml found")?;
        if let Some(version_req) = &ambient_toml.package.ambient_version {
            return get_version_satisfying_req(settings, version_req);
        }
    }
    match &settings.default_runtime {
        Some(version) => Ok(RuntimeVersion::without_builds(version.clone())),
        None => {
            anyhow::bail!("No default runtime version set")
        }
    }
}

fn set_default_runtime(settings: &mut Settings, version: &RuntimeVersion) -> anyhow::Result<()> {
    version.install()?;
    settings.default_runtime = Some(version.version.clone());
    settings.save()?;
    println!(
        "The default runtime version is now {}",
        version.version.to_string()
    );
    Ok(())
}

fn version_manager_main(
    package_path: &Option<PackagePath>,
    mut settings: Settings,
) -> anyhow::Result<()> {
    let args = Args::parse();

    match args.command {
        Commands::Runtime(RuntimeCommands::ListAll) => {
            for build in get_versions(VersionsFilter {
                include_private: true,
                include_nightly: true,
            })? {
                println!("{}", build.version);
            }
        }
        Commands::Runtime(RuntimeCommands::ListInstalled) => {
            for (version, _) in list_installed_runtimes()? {
                println!("{}", version);
            }
        }
        Commands::Runtime(RuntimeCommands::Install { version }) => {
            let runtime_version = get_version(&version)?;
            runtime_version.install()?;
        }
        Commands::Runtime(RuntimeCommands::SetDefault { version }) => {
            let runtime_version = get_version(&version)?;
            set_default_runtime(&mut settings, &runtime_version)?;
        }
        Commands::Runtime(RuntimeCommands::SetLocal { version }) => {
            package_path
                .as_ref()
                .context("No local package found")?
                .set_runtime(&semver::Version::parse(&version)?)?;
        }
        Commands::Runtime(RuntimeCommands::UpdateDefault) => {
            let version = get_latest_remote_version_for_train(settings.release_train(), false)?;
            set_default_runtime(&mut settings, &version)?;
        }
        Commands::Runtime(RuntimeCommands::UpdateLocal) => {
            let package_path = package_path.as_ref().context("No local package found")?;
            let ambient_toml = package_path
                .ambient_toml()
                .get_content()?
                .context("No ambient.toml found")?;
            let release_train = ambient_toml
                .package
                .ambient_version
                .map(|v| ReleaseTrain::from_version_req(&v))
                .unwrap_or(ReleaseTrain::Stable);
            let version = get_latest_remote_version_for_train(release_train, false)?;
            package_path.set_runtime(&version.version)?;
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

fn runtime_exec(
    mut settings: Settings,
    package_path: &Option<PackagePath>,
    args: Vec<String>,
) -> anyhow::Result<()> {
    if settings.default_runtime.is_none() {
        println!("No default runtime version set, installing latest stable version");
        let version = get_latest_remote_version_for_train(ReleaseTrain::Stable, true)?;
        set_default_runtime(&mut settings, &version)?;
    }
    let version = get_current_runtime(&settings, &package_path)?;
    version.install()?;
    let mut process = std::process::Command::new(version.exe_path()?)
        .args(args)
        .spawn()?;
    process.wait()?;
    Ok(())
}

fn main() -> anyhow::Result<()> {
    env_logger::init();

    let settings = if settings_path()?.exists() {
        Settings::load()?
    } else {
        Settings::default()
    };

    let args: Vec<String> = std::env::args().skip(1).collect();
    let package_path = PackagePath::get(&args);
    if args.get(0) == Some(&"runtime".to_string()) {
        version_manager_main(&package_path, settings)?;
    } else if args.get(0) == Some(&"--help".to_string()) {
        runtime_exec(settings, &package_path, args)?;
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
        if args.get(0) == Some(&"--version".to_string()) {
            if let Some(package) = &package_path {
                println!("Using package at {:?}", package.0);
            } else {
                println!("Using global runtime version");
            }
        }
        runtime_exec(settings, &package_path, args)?;
    }

    Ok(())
}
