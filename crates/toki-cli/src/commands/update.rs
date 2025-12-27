//! Self-update functionality for toki
//!
//! Checks GitHub Releases for updates and automatically installs new versions.

use anyhow::{Context, Result, bail};
use std::env;
use std::fs;
use std::io::Read;
use std::process::Command;

const GITHUB_REPO: &str = "RikaiDev/toki";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// GitHub Release API response
#[derive(Debug, serde::Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Debug, serde::Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

/// Check for updates and optionally install
pub fn handle_update_command(check_only: bool) -> Result<()> {
    println!("Current version: v{CURRENT_VERSION}");
    println!("Checking for updates...\n");

    let latest = fetch_latest_release()?;
    let latest_version = latest.tag_name.trim_start_matches('v');

    if !is_newer_version(latest_version, CURRENT_VERSION) {
        println!("You're already running the latest version!");
        return Ok(());
    }

    println!("New version available: v{latest_version}");

    if check_only {
        println!("\nRun `toki update` to install the update.");
        return Ok(());
    }

    // Find the right asset for this platform
    let asset_name = get_asset_name()?;
    let asset = latest
        .assets
        .iter()
        .find(|a| a.name == asset_name)
        .context(format!("No release asset found for platform: {asset_name}"))?;

    println!("\nDownloading {asset_name}...");
    let binary_data = download_asset(&asset.browser_download_url)?;

    // Get current executable path
    let current_exe = env::current_exe().context("Failed to get current executable path")?;
    let backup_path = current_exe.with_extension("backup");

    println!("Installing update...");

    // Backup current binary
    if backup_path.exists() {
        fs::remove_file(&backup_path).ok();
    }
    fs::copy(&current_exe, &backup_path).context("Failed to backup current binary")?;

    // Extract and replace
    let temp_dir = tempfile::tempdir().context("Failed to create temp directory")?;
    let archive_path = temp_dir.path().join(&asset_name);
    fs::write(&archive_path, &binary_data).context("Failed to write downloaded archive")?;

    // Extract .tar.xz
    let output = Command::new("tar")
        .args(["xf", archive_path.to_str().unwrap()])
        .current_dir(temp_dir.path())
        .output()
        .context("Failed to extract archive")?;

    if !output.status.success() {
        bail!(
            "Failed to extract archive: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // Find the extracted binary
    let extracted_binary = temp_dir.path().join("toki");
    if !extracted_binary.exists() {
        // Try looking in subdirectory
        let entries: Vec<_> = fs::read_dir(temp_dir.path())?
            .filter_map(|e| e.ok())
            .collect();

        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                let bin = path.join("toki");
                if bin.exists() {
                    replace_binary(&bin, &current_exe)?;
                    return finish_update(&backup_path, latest_version);
                }
            }
        }
        bail!("Could not find extracted toki binary");
    }

    replace_binary(&extracted_binary, &current_exe)?;
    finish_update(&backup_path, latest_version)
}

fn replace_binary(src: &std::path::Path, dest: &std::path::Path) -> Result<()> {
    // On Unix, we need to handle the case where the binary is running
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        // Copy new binary to a temp location next to destination
        let temp_dest = dest.with_extension("new");
        fs::copy(src, &temp_dest).context("Failed to copy new binary")?;

        // Set executable permission
        let mut perms = fs::metadata(&temp_dest)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&temp_dest, perms)?;

        // Atomic rename
        fs::rename(&temp_dest, dest).context("Failed to replace binary")?;
    }

    #[cfg(not(unix))]
    {
        fs::copy(src, dest).context("Failed to replace binary")?;
    }

    Ok(())
}

fn finish_update(backup_path: &std::path::Path, new_version: &str) -> Result<()> {
    println!("\nSuccessfully updated to v{new_version}!");
    println!("Backup saved to: {}", backup_path.display());

    // Restart daemon if running
    restart_daemon()?;

    Ok(())
}

fn restart_daemon() -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let home = dirs::home_dir().context("Failed to get home directory")?;
        let plist_path = home.join("Library/LaunchAgents/dev.rikai.toki.plist");

        if plist_path.exists() {
            println!("\nRestarting daemon...");

            let _ = Command::new("launchctl")
                .args(["unload", plist_path.to_str().unwrap()])
                .output();

            let output = Command::new("launchctl")
                .args(["load", plist_path.to_str().unwrap()])
                .output()?;

            if output.status.success() {
                println!("Daemon restarted successfully.");
            } else {
                println!("Warning: Failed to restart daemon. Please run `toki start` manually.");
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        let _ = Command::new("systemctl")
            .args(["--user", "restart", "toki.service"])
            .output();
        println!("Daemon restart requested.");
    }

    Ok(())
}

fn fetch_latest_release() -> Result<Release> {
    let url = format!("https://api.github.com/repos/{GITHUB_REPO}/releases/latest");

    let body = ureq::get(&url)
        .header("User-Agent", &format!("toki/{CURRENT_VERSION}"))
        .call()
        .context("Failed to fetch release info")?
        .into_body()
        .read_to_string()
        .context("Failed to read response body")?;

    let release: Release = serde_json::from_str(&body).context("Failed to parse release info")?;
    Ok(release)
}

fn download_asset(url: &str) -> Result<Vec<u8>> {
    let response = ureq::get(url)
        .header("User-Agent", &format!("toki/{CURRENT_VERSION}"))
        .call()
        .context("Failed to download asset")?;

    let len = response
        .headers()
        .get("Content-Length")
        .and_then(|s| s.to_str().ok())
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(10_000_000);

    let mut data = Vec::with_capacity(len);
    response
        .into_body()
        .into_reader()
        .take(50_000_000) // Max 50MB
        .read_to_end(&mut data)
        .context("Failed to read download")?;

    Ok(data)
}

fn get_asset_name() -> Result<String> {
    let arch = env::consts::ARCH;
    let os = env::consts::OS;

    let target = match (os, arch) {
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        _ => bail!("Unsupported platform: {os}-{arch}"),
    };

    Ok(format!("toki-cli-{target}.tar.xz"))
}

fn is_newer_version(latest: &str, current: &str) -> bool {
    let parse_version = |v: &str| -> (u32, u32, u32) {
        let parts: Vec<u32> = v
            .split('.')
            .filter_map(|s| s.parse().ok())
            .collect();
        (
            parts.first().copied().unwrap_or(0),
            parts.get(1).copied().unwrap_or(0),
            parts.get(2).copied().unwrap_or(0),
        )
    };

    let latest_parts = parse_version(latest);
    let current_parts = parse_version(current);

    latest_parts > current_parts
}

/// Background update check (for daemon startup)
#[allow(dead_code)]
pub fn check_for_updates_background() {
    std::thread::spawn(|| {
        if let Ok(latest) = fetch_latest_release() {
            let latest_version = latest.tag_name.trim_start_matches('v');
            if is_newer_version(latest_version, CURRENT_VERSION) {
                eprintln!(
                    "\n[toki] New version available: v{latest_version} (current: v{CURRENT_VERSION})"
                );
                eprintln!("[toki] Run `toki update` to install\n");
            }
        }
    });
}
