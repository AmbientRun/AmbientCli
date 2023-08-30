use anyhow::Context;
use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};
use termion::{color, style};
use toml::Value;
use zip::read::ZipArchive;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let options: Vec<&str> = vec!["v0.2.1", "nightly"];
    let mut show_help = false;
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.get(0) == Some(&"--help".to_string()) || args.get(0) == None {
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
                "v0.2.1" => {
                    set_version("stable v0.2.1".to_string());
                }
                "nightly" => {
                    println!();

                    let d = inquire::DateSelect::new("Nightly version date:")
                        .with_default(chrono::Utc::now().date_naive())
                        .with_min_date(chrono::NaiveDate::from_ymd_opt(2023, 8, 30).unwrap())
                        .with_max_date(chrono::Utc::now().date_naive()) //.pred_opt()
                        .prompt()
                        .unwrap();
                    println!(
                        "You selected nightly version: {}",
                        d.format("%Y-%m-%d").to_string()
                    );
                    println!();

                    set_version(format!("nightly {}", d.format("%Y-%m-%d").to_string()));
                }
                _ => anyhow::bail!("Unsupported version"),
            },
            Err(e) => anyhow::bail!(e),
        }
    } else if args.get(0) == Some(&"--version".to_string())
        || args.get(0) == Some(&"-V".to_string())
    {
        println!("ambl version: {}", env!("CARGO_PKG_VERSION"));
        println!(
            "{}{}Note that this is just the version of the launcher.{}",
            color::Bg(color::Magenta),
            color::Fg(color::Yellow),
            style::Reset
        );
        println!(
            "You can select Ambient runtime version with {}{}`ambl set-default`{}",
            color::Bg(color::Magenta),
            color::Fg(color::Yellow),
            style::Reset
        );
    } else {
        let version = get_version();

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

        let is_stable = version.split(" ").collect::<Vec<&str>>()[0] == "stable";
        let version = version.split(" ").collect::<Vec<&str>>()[1];
        // check if the version is installed
        let home_dir = dirs::home_dir().expect("Failed to get home directory.");
        let ambient_dir = home_dir.join(".ambient");
        let runtime_dir = if is_stable {
            match std::env::consts::OS {
                "macos" => ambient_dir.join(format!("ambient-{}", version.replace(".", "-"))),
                "ubuntu" => ambient_dir.join(format!("ambient-{}", version.replace(".", "-"))),
                _ => anyhow::bail!("Unsupported OS"),
            }
        } else {
            match std::env::consts::OS {
                "macos" => {
                    ambient_dir.join(format!("ambient-nightly-{}", version.replace(".", "-")))
                }
                "ubuntu" => {
                    ambient_dir.join(format!("ambient-nightly-{}", version.replace(".", "-")))
                }
                _ => anyhow::bail!("Unsupported OS"),
            }
        };

        if !runtime_dir.exists() || fs::metadata(&runtime_dir)?.len() == 0 {
            if is_stable {
                println!("Downloading stable version...");
                download_stable(version.to_string()).await?;
                println!("Downloaded stable version at {:?}", runtime_dir);
            } else {
                println!("Downloading nightly version...");
                download_nightly(version.to_string()).await?;
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
    }
    Ok(())
}

fn wget_is_available() -> bool {
    match std::process::Command::new("wget").arg("--version").output() {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}

pub async fn download_stable(version: String) -> anyhow::Result<String> {
    tokio::task::spawn_blocking(move || {
        let url = match std::env::consts::OS {
            "windows" => format!("https://github.com/AmbientRun/Ambient/releases/download/{}/ambient-x86_64-pc-windows-msvc.zip", version),
            "macos" => format!(
                "https://github.com/AmbientRun/Ambient/releases/download/{}/ambient-aarch64-apple-darwin.zip", version),
            "ubuntu" => format!("https://github.com/AmbientRun/Ambient/releases/download/{}/ambient-x86_64-unknown-linux-gnu.zip", version),
            _ => anyhow::bail!("Unsupported OS"),
        };

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

        // Extract zip
        let mut archive = ZipArchive::new(fs::File::open(&zip_path)?)?;
        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;

            // TODO: windows should be different?
            let outpath = dest_folder.join(format!("ambient-{}", version.replace(".", "-")));

            if (&*file.name()).ends_with('/') {
                fs::create_dir_all(&outpath)?;
            } else {
                if let Some(p) = outpath.parent() {
                    if !p.exists() {
                        fs::create_dir_all(&p)?;
                    }
                }
                let mut outfile = fs::File::create(&outpath)?;
                std::io::copy(&mut file, &mut outfile)?;
            }
        }

        // Set permissions (if needed)
        let path = dest_folder.join(format!("ambient-{}", version.replace(".", "-")));
        // println!("path: {:?}", &path);
        if std::env::consts::OS != "windows" {
            let output = std::process::Command::new("chmod")
                .arg("+x")
                .arg(&path)  // replace "ambient" with your binary name
                .output()
                .expect("Failed to execute chmod");

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                anyhow::bail!("Failed to set permissions: {}", stderr);
            }
        }

        Ok(format!("{:?}", &path).replace("\"", ""))  // replace "ambient" with your binary name
    })
    .await
    .unwrap()
}

pub async fn download_nightly(date: String) -> anyhow::Result<String> {
    tokio::task::spawn_blocking(move || {
        let url = match std::env::consts::OS {
            "macos" => format!("https://storage.googleapis.com/ambient-artifacts/ambient-nightly-build/{date}/macos-latest/ambient-aarch64-apple-darwin.zip"),
            "ubuntu" => format!("https://storage.googleapis.com/ambient-artifacts/ambient-nightly-build/{date}/ubuntu-latest/ambient-x86_64-unknown-linux-gnu.zip"),
            _ => anyhow::bail!("Unsupported OS"),
        };


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

        // Extract zip
        let mut archive = ZipArchive::new(fs::File::open(&zip_path)?)?;
        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;

            // TODO: windows should be different?
            let outpath = dest_folder.join(format!("ambient-nightly-{}", date));

            if (&*file.name()).ends_with('/') {
                fs::create_dir_all(&outpath)?;
            } else {
                if let Some(p) = outpath.parent() {
                    if !p.exists() {
                        fs::create_dir_all(&p)?;
                    }
                }
                let mut outfile = fs::File::create(&outpath)?;
                std::io::copy(&mut file, &mut outfile)?;
            }
        }

        // Set permissions (if needed)
        let path = dest_folder.join(format!("ambient-nightly-{}", date));
        // println!("path: {:?}", &path);
        if std::env::consts::OS != "windows" {
            let output = std::process::Command::new("chmod")
                .arg("+x")
                .arg(&path)  // replace "ambient" with your binary name
                .output()
                .expect("Failed to execute chmod");

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                anyhow::bail!("Failed to set permissions: {}", stderr);
            }
        }

        Ok(format!("{:?}", &path).replace("\"", ""))  // replace "ambient" with your binary name
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
        config.insert(
            "default".to_string(),
            Value::String("stable v0.2.1".to_string()),
        );

        let toml_content =
            toml::to_string_pretty(&config).expect("Failed to serialize TOML content.");

        file.write_all(toml_content.as_bytes())
            .expect("Failed to write to config.toml file.");
        println!("config.toml file created at {:?}", config_path);
        return "stable v0.2.1".to_string();
    } else {
        let config_content = fs::read_to_string(&config_path).expect("Failed to read config file.");
        let config: Value = toml::from_str(&config_content).expect("Failed to parse config file.");
        let default_version = config["default"].as_str().unwrap();
        return default_version.to_string();
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
