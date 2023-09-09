use anyhow::Context;
use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};
use termion::{color, style};
use toml::Value;
use zip::read::ZipArchive;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let options: Vec<&str> = vec!["0.2.1", "0.3.0-nightly"];
    let mut show_help = false;
    let args: Vec<String> = std::env::args().skip(1).collect();

    let output = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .stdout(Stdio::piped())
        .output()
        .expect("Failed to execute rustup command");

    let installed_targets = String::from_utf8_lossy(&output.stdout);

    if !installed_targets.contains("wasm32-wasi") {
        println!("Installing wasm32-wasi target... This is a one-time operation.");
        let mut child = Command::new("rustup")
            .arg("target")
            .arg("add")
            .arg("--toolchain")
            .arg("stable")
            .arg("wasm32-wasi")
            .stdout(Stdio::inherit()) // Forward stdout directly
            .stderr(Stdio::inherit()) // Forward stderr directly
            .spawn()
            .expect("Failed to execute command");

        let _status = child.wait().expect("Failed to wait on child");
    }

    if args.get(0) == Some(&"--help".to_string()) || args.get(0).is_none() {
        show_help = true;
    }

    if args.get(0) == Some(&"set-default".to_string()) {
        let ans = inquire::Select::new(
            "Which ambient runtime version would you like to use?",
            options,
        )
        .prompt();
        match ans {
            Ok(choice) => match choice {
                "0.2.1" => {
                    set_version("0.2.1".to_string());
                }
                "0.3.0-nightly" => {
                    println!();

                    let d = inquire::DateSelect::new("Nightly version date:")
                        .with_default(chrono::Utc::now().date_naive())
                        .with_min_date(chrono::NaiveDate::from_ymd_opt(2023, 9, 7).unwrap())
                        .with_max_date(chrono::Utc::now().date_naive()) //.pred_opt()
                        .prompt()
                        .unwrap();
                    println!("You selected nightly version: {}", d.format("%Y-%m-%d"));
                    println!();

                    set_version(format!("0.3.0-nightly-{}", d.format("%Y-%m-%d")));
                }
                _ => anyhow::bail!("Unsupported version"),
            },
            Err(e) => anyhow::bail!(e),
        }
    } else {
        if args.get(0) == Some(&"--version".to_string()) || args.get(0) == Some(&"-V".to_string()) {
            println!("ambl version: {}", env!("CARGO_PKG_VERSION"));
            println!(
                "You can select Ambient runtime version with {}{}`ambl set-default`{}",
                style::Bold,
                color::Fg(color::Yellow),
                style::Reset
            );
        }
        let version = if args.contains(&"run".to_string())
            || args.contains(&"deploy".to_string())
            || args.contains(&"build".to_string())
        {
            // read the ambient.toml file
            let current_dir = std::env::current_dir().expect("Failed to get current directory.");
            let ambient_toml_path = current_dir.join("ambient.toml");
            if !ambient_toml_path.exists() {
                anyhow::bail!("ambient.toml file not found in the current directory.");
            }
            let ambient_toml_content =
                fs::read_to_string(&ambient_toml_path).expect("Failed to read ambient.toml file.");
            let ambient_toml: Value =
                toml::from_str(&ambient_toml_content).expect("Failed to parse ambient.toml file.");
            let version_str = ambient_toml["package"]["ambient_version"].as_str();
            if version_str.is_none() {
                get_version()
            } else {
                match version_str {
                    Some(version_str) => version_str.to_string(),
                    None => {
                        eprintln!("Invalid ambient_version in ambient.toml file. Use the default selected version in ambl.");
                        get_version()
                    }
                }
            }
        } else {
            get_version()
        };

        println!(
            "{}{}{}Current ambient runtime version selected for ambl:{}\n\t{}",
            style::Bold,
            style::Underline,
            color::Fg(color::Magenta),
            style::Reset,
            version,
        );

        println!(
            "{}{}{}Select Ambient runtime version:{}\n\tambl set-default",
            style::Bold,
            style::Underline,
            color::Fg(color::Magenta),
            style::Reset
        );

        if show_help {
            println!(
                "{}{}{}The usage info below applies only to `ambl`, e.g.:{}\n\tambl new\n",
                style::Bold,
                style::Underline,
                color::Fg(color::Magenta),
                style::Reset
            );
        }

        if args.get(0) == Some(&"--version".to_string()) || args.get(0) == Some(&"-V".to_string()) {
            println!(
                "{}{}{}Local ambient runtime version (should be the same as the selected):{}",
                style::Bold,
                style::Underline,
                color::Fg(color::Blue),
                style::Reset
            );
        }

        let is_stable = !version.contains("nightly");

        // check if the version is installed
        let home_dir = dirs::home_dir().expect("Failed to get home directory.");
        let ambient_dir = home_dir.join(".ambient");
        let runtime_dir = match std::env::consts::OS {
            "macos" => ambient_dir.join(format!("ambient-{}", version.replace('.', "-"))),
            "ubuntu" => ambient_dir.join(format!("ambient-{}", version.replace('.', "-"))),
            _ => anyhow::bail!("Unsupported OS"),
        };

        if !runtime_dir.exists() || fs::metadata(&runtime_dir)?.len() == 0 {
            let os = std::env::consts::OS;
            if is_stable {
                println!("Downloading stable version... This will only happen once.");
                download(
                    get_url(version.to_string(), os.to_string())?,
                    Version::Stable(version.to_string()),
                )
                .await?;
                println!("Downloaded stable version at {:?}", runtime_dir);
            } else {
                println!("Downloading nightly version... This will only happen once.");
                download(
                    get_url(version.to_string(), os.to_string())?,
                    Version::Nightly(version.to_string()),
                )
                .await?;
                println!("Downloaded nightly version at {:?}", runtime_dir);
            }
        }

        // run the runtime with args
        let all_args: Vec<String> = std::env::args().skip(1).collect();
        let mut child = Command::new(&runtime_dir)
            .args(&all_args)
            .stdout(Stdio::inherit()) // Forward stdout directly
            .stderr(Stdio::inherit()) // Forward stderr directly
            .spawn()
            .expect("Failed to execute command");

        let _status = child.wait().expect("Failed to wait on child");

        if args.contains(&"new".to_string()) {
            let folder = args.get(1).context("Missing folder name")?;
            let current_dir = std::env::current_dir().expect("Failed to get current directory.");
            let ambient_toml_path = current_dir.join(format!("{folder}/ambient.toml"));
            if !ambient_toml_path.exists() {
                println!("ambient.toml file not found at {:?}", ambient_toml_path);
            } else {
                // read the ambient.toml file and add the ambient_version
                let ambient_toml_content = fs::read_to_string(&ambient_toml_path)
                    .expect("Failed to read ambient.toml file.");
                let mut ambient_toml: Value = toml::from_str(&ambient_toml_content)
                    .expect("Failed to parse ambient.toml file.");
                // create [ambient_version] under [package]
                ambient_toml["package"].as_table_mut().unwrap().insert(
                    "ambient_version".to_string(),
                    Value::String(version.to_string()),
                );

                // Serialize the TOML contents
                let toml_content = toml::to_string_pretty(&ambient_toml)
                    .expect("Failed to serialize TOML content.");
                // Write the updated TOML content back to the file
                fs::write(&ambient_toml_path, toml_content.as_bytes())
                    .expect("Failed to write to ambient.toml file.");
            }
        }
    }
    Ok(())
}

fn wget_is_available() -> bool {
    match std::process::Command::new("wget").arg("--version").output() {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}

/// Get the download URL for a given version and OS
/// e.g. `get_url("0.2.1", "macos")`
pub fn get_url(version: String, os: String) -> anyhow::Result<String> {
    let v = semver::Version::parse(&version).context("Failed to parse version")?;
    let s = if !version.contains("nightly") {
        match os.as_str() {
            "windows" => format!("https://github.com/AmbientRun/Ambient/releases/download/v{}.{}.{}/ambient-x86_64-pc-windows-msvc.zip", v.major, v.minor, v.patch),
            "macos" => format!("https://github.com/AmbientRun/Ambient/releases/download/v{}.{}.{}/ambient-aarch64-apple-darwin.zip", v.major, v.minor, v.patch),
            "ubuntu" => format!("https://github.com/AmbientRun/Ambient/releases/download/v{}.{}.{}/ambient-x86_64-unknown-linux-gnu.zip", v.major, v.minor, v.patch),
            _ => anyhow::bail!("Unsupported OS"),
        }
    } else {
        // e.g. 0.3.0.nightly-2023-08-31
        let version = v.pre;
        let date = version.replace("nightly-", "");
        match os.as_str() {
            "macos" => format!("https://storage.googleapis.com/ambient-artifacts/ambient-nightly-build/{date}/macos-latest/ambient-aarch64-apple-darwin.zip"),
            "ubuntu" => format!("https://storage.googleapis.com/ambient-artifacts/ambient-nightly-build/{date}/ubuntu-22.04/ambient-x86_64-unknown-linux-gnu.zip"),
            "windows" => format!("https://storage.googleapis.com/ambient-artifacts/ambient-nightly-build/{date}/windows-latest/ambient-x86_64-pc-windows-msvc.zip"),
            _ => anyhow::bail!("Unsupported OS"),
        }
    };

    Ok(s)
}

/// The version of the runtime
pub enum Version {
    /// The stable version
    Stable(String),
    /// The nightly version
    Nightly(String),
}

/// Download the runtime
pub async fn download(url: String, v: Version) -> anyhow::Result<String> {
    tokio::task::spawn_blocking(move || {
        let home_dir = dirs::home_dir().expect("Failed to get home directory.");
        let dest_folder = home_dir.join(".ambient");
        let zip_path = dest_folder.join(url.split('/').last().unwrap_or("unknown.zip"));

        if wget_is_available() {
            let status = std::process::Command::new("wget")
                .arg("-O")
                .arg(zip_path.to_string_lossy().to_string())
                .arg(&url)
                .status()?;

            if !status.success() {
                anyhow::bail!("wget failed");
            }
        } else {
            // Download zip
            let mut response = reqwest::blocking::get(&url).context("Failed to download")?;
            let mut file = fs::File::create(&zip_path)?;
            response.copy_to(&mut file)?;
        }

        // TODO: windows should be different?
        let outpath = match v {
            Version::Stable(version) => {
                dest_folder.join(format!("ambient-{}", version.replace('.', "-")))
            }
            Version::Nightly(version) => {
                dest_folder.join(format!("ambient-{}", version.replace('.', "-")))
            }
        };

        // Extract zip
        let mut archive = ZipArchive::new(fs::File::open(&zip_path)?)?;
        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;

            if (*file.name()).ends_with('/') {
                fs::create_dir_all(&outpath)?;
            } else {
                if let Some(p) = outpath.parent() {
                    if !p.exists() {
                        fs::create_dir_all(p)?;
                    }
                }
                let mut outfile = fs::File::create(&outpath)?;
                std::io::copy(&mut file, &mut outfile)?;
            }
        }

        // Set permissions
        if std::env::consts::OS != "windows" {
            let output = std::process::Command::new("chmod")
                .arg("+x")
                .arg(&outpath)
                .output()
                .expect("Failed to execute chmod");

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                anyhow::bail!("Failed to set permissions: {}", stderr);
            }
        }

        Ok(format!("{:?}", &outpath).replace('\"', ""))
    })
    .await
    .unwrap()
}

fn get_version() -> String {
    let home_dir = dirs::home_dir().expect("Failed to get home directory.");
    let ambient_dir = home_dir.join(".ambient");
    let config_path = ambient_dir.join("config.toml");
    if !ambient_dir.exists() {
        fs::create_dir_all(&ambient_dir).expect("Failed to create .ambient directory.");
    }
    if !config_path.exists() {
        let mut file =
            std::fs::File::create(&config_path).expect("Failed to create config.toml file.");
        let mut config = toml::value::Table::new();
        config.insert("default".to_string(), Value::String("0.2.1".to_string()));

        let toml_content =
            toml::to_string_pretty(&config).expect("Failed to serialize TOML content.");

        file.write_all(toml_content.as_bytes())
            .expect("Failed to write to config.toml file.");
        println!("config.toml file created at {:?}", config_path);
        "0.2.1".to_string()
    } else {
        let config_content = fs::read_to_string(&config_path).expect("Failed to read config file.");
        let config: Value = toml::from_str(&config_content).expect("Failed to parse config file.");
        let default_version = config["default"].as_str().unwrap();
        default_version.to_string()
    }
}

fn set_version(version: String) {
    let home_dir = dirs::home_dir().expect("Failed to get home directory.");
    let ambient_dir = home_dir.join(".ambient");
    let config_path = ambient_dir.join("config.toml");

    // Create the directory if it doesn't exist
    if !ambient_dir.exists() {
        fs::create_dir_all(&ambient_dir).expect("Failed to create .ambient directory.");
    }

    let mut config = if config_path.exists() {
        // If the config.toml file already exists, read its contents
        let config_content = fs::read_to_string(&config_path).expect("Failed to read config file.");
        toml::from_str(&config_content).expect("Failed to parse config file.")
    } else {
        // If the config.toml file doesn't exist, create a new empty TOML table
        Value::Table(toml::value::Table::new())
    };

    // Update or set the version
    if let Value::Table(table) = &mut config {
        table.insert("default".to_string(), Value::String(version.clone()));
    } else {
        panic!("The root of the TOML should be a table");
    }

    // Serialize the TOML contents
    let toml_content = toml::to_string_pretty(&config).expect("Failed to serialize TOML content.");

    // Write the updated TOML content back to the file
    fs::write(&config_path, toml_content.as_bytes()).expect("Failed to write to config.toml file.");

    println!("Set version to {} in {:?}", version, config_path);
}
