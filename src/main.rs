use anyhow::Context;
use clap::Parser;
use futures::StreamExt;
use itertools::Itertools;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct BucketList {
    items: Vec<BucketItem>,
}
#[derive(Debug, Deserialize)]
struct BucketItem {
    name: String,
    mediaLink: String,
}

#[derive(Debug)]
struct RuntimeVersion {
    version: semver::Version,
    builds: Vec<Build>,
}
#[derive(Debug)]
struct Build {
    os: String,
    url: String,
}

fn version_from_path(path: &str) -> anyhow::Result<semver::Version> {
    let version = path.split("/").nth(1).context("Invalid path")?;
    Ok(semver::Version::parse(version)?)
}

async fn get_builds() -> anyhow::Result<Vec<RuntimeVersion>> {
    let builds = reqwest::get("https://storage.googleapis.com/storage/v1/b/ambient-artifacts/o?prefix=ambient-builds%2F&alt=json")
        .await?
        .json::<BucketList>()
        .await?;
    let builds = builds
        .items
        .into_iter()
        .filter_map(|b| Some((version_from_path(&b.name).ok()?, b)))
        .collect_vec();
    let mut versions = Vec::new();
    for (version, builds) in builds.into_iter().group_by(|x| x.0.clone()).into_iter() {
        versions.push(RuntimeVersion {
            version,
            builds: builds
                .map(|(_, build)| Build {
                    os: build.name.split("/").nth(2).unwrap().to_string(),
                    url: build.mediaLink,
                })
                .collect(),
        });
    }
    // let mut builds = builds
    //     .items
    //     .into_iter()
    //     .filter_map(|build| {
    //         let version = build.name.split("/").nth(1)?;
    //         semver::Version::parse(version).ok()
    //     })
    //     .sorted()
    //     .collect::<Vec<_>>();
    // let versions = Vec::new();
    // for ()
    // builds.sort();
    // builds.dedup();
    Ok(versions)
}

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
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    match args.command {
        Commands::Runtime(RuntimeCommands::List) => {
            for build in get_builds().await? {
                println!("{:?}", build);
            }
        }
        Commands::Runtime(RuntimeCommands::Install { version }) => {}
        Commands::Variant(args) => {
            println!("args: {:?}", args);
        }
    }

    Ok(())
}
