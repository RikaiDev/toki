//! Initialize toki with complete system setup
//!
//! Handles database initialization, permission guidance, and auto-start configuration

use anyhow::{Context, Result};
use std::fs;
use std::process::Command;
use toki_storage::{default_key_path, generate_key, save_key_to_file, Database};

/// Initialize toki with complete setup
///
/// # Errors
///
/// Returns an error if key generation, file operations, or database initialization fails
pub fn init_command(enable_encryption: bool) -> Result<()> {
    println!("Initializing Toki Time Tracking...\n");

    // Step 1: Database setup
    println!("Step 1/3: Database Setup");
    println!("{}", "-".repeat(40));
    setup_database(enable_encryption)?;

    // Step 2: Platform-specific permissions
    println!("\nStep 2/3: System Permissions");
    println!("{}", "-".repeat(40));
    show_permission_guidance();

    // Step 3: Auto-start configuration
    println!("\nStep 3/3: Auto-start Configuration");
    println!("{}", "-".repeat(40));
    if let Err(e) = setup_autostart() {
        println!("Warning: Could not configure auto-start: {e}");
        println!("You can start toki manually with: toki start");
    }

    println!("\n========================================");
    println!("  Setup Complete!");
    println!("========================================");
    println!("\nToki is ready. Check status with: toki status");

    Ok(())
}

fn setup_database(enable_encryption: bool) -> Result<()> {
    if enable_encryption {
        println!("Setting up encrypted database...");

        let key_path = default_key_path();

        if key_path.exists() {
            println!("Encryption key already exists at: {}", key_path.display());
            println!("Using existing key.");

            // Just verify database works
            let _ = Database::new(None)?;
        } else {
            let key = generate_key();
            save_key_to_file(&key, &key_path)?;

            println!("Encryption key saved to: {}", key_path.display());
            println!("Keep this file safe!");

            let db = Database::new_with_encryption(None, Some(key))?;
            drop(db);
        }

        println!("Encrypted database ready.");
    } else {
        println!("Setting up database (unencrypted)...");

        let db = Database::new(None)?;
        drop(db);

        println!("Database ready.");
        println!("Tip: Run 'toki init --encrypt' for encrypted storage.");
    }

    Ok(())
}

fn show_permission_guidance() {
    #[cfg(target_os = "macos")]
    {
        println!("Toki needs Accessibility permission to read window titles.");
        println!();
        println!("Please grant permission:");
        println!("  1. Open System Settings > Privacy & Security > Accessibility");
        println!("  2. Click the + button");
        println!("  3. Add your terminal app (Terminal, iTerm2, Warp, etc.)");
        println!();
        println!("Without this, toki cannot detect which app you're using.");
    }

    #[cfg(target_os = "linux")]
    {
        println!("No special permissions required on Linux.");
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        println!("Platform-specific permissions may be required.");
    }
}

// ============================================================================
// Auto-start configuration
// ============================================================================

#[cfg(target_os = "macos")]
fn setup_autostart() -> Result<()> {
    let toki_path = std::env::current_exe().context("Failed to get executable path")?;
    let data_dir = toki_core::config::get_data_dir()?;
    let home_dir = dirs::home_dir().context("Failed to get home directory")?;

    let launch_agents_dir = home_dir.join("Library/LaunchAgents");
    let plist_path = launch_agents_dir.join("dev.rikai.toki.plist");

    // Ensure directory exists
    fs::create_dir_all(&launch_agents_dir)?;

    // Generate plist
    let plist_content = generate_macos_plist(&toki_path, &data_dir);

    // Unload existing if present
    if plist_path.exists() {
        let _ = Command::new("launchctl")
            .args(["unload", plist_path.to_str().unwrap()])
            .output();
    }

    // Write plist
    fs::write(&plist_path, plist_content)?;
    println!("Created: {}", plist_path.display());

    // Load service
    let output = Command::new("launchctl")
        .args(["load", plist_path.to_str().unwrap()])
        .output()?;

    if output.status.success() {
        println!("Auto-start enabled. Toki will start on login.");
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        println!("Warning: launchctl load failed: {stderr}");
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn generate_macos_plist(toki_path: &std::path::Path, data_dir: &std::path::Path) -> String {
    let log_path = data_dir.join("toki.log");
    let err_log_path = data_dir.join("toki.err.log");

    format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>dev.rikai.toki</string>

    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
        <string>daemon-internal-start</string>
    </array>

    <key>RunAtLoad</key>
    <true/>

    <key>KeepAlive</key>
    <dict>
        <key>SuccessfulExit</key>
        <false/>
    </dict>

    <key>StandardOutPath</key>
    <string>{}</string>

    <key>StandardErrorPath</key>
    <string>{}</string>

    <key>EnvironmentVariables</key>
    <dict>
        <key>PATH</key>
        <string>/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin</string>
    </dict>
</dict>
</plist>
"#,
        toki_path.display(),
        log_path.display(),
        err_log_path.display()
    )
}

#[cfg(target_os = "linux")]
fn setup_autostart() -> Result<()> {
    let toki_path = std::env::current_exe().context("Failed to get executable path")?;
    let home_dir = dirs::home_dir().context("Failed to get home directory")?;

    let systemd_dir = home_dir.join(".config/systemd/user");
    let service_path = systemd_dir.join("toki.service");

    // Ensure directory exists
    fs::create_dir_all(&systemd_dir)?;

    // Generate service file
    let service_content = format!(r#"[Unit]
Description=Toki Time Tracking Daemon
After=default.target

[Service]
Type=simple
ExecStart={} daemon-internal-start
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
"#,
        toki_path.display()
    );

    fs::write(&service_path, service_content)?;
    println!("Created: {}", service_path.display());

    // Reload, enable, and start
    let _ = Command::new("systemctl").args(["--user", "daemon-reload"]).output();
    let _ = Command::new("systemctl").args(["--user", "enable", "toki.service"]).output();
    let _ = Command::new("systemctl").args(["--user", "start", "toki.service"]).output();

    println!("Auto-start enabled. Toki will start on login.");

    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn setup_autostart() -> Result<()> {
    println!("Auto-start not supported on this platform.");
    println!("Please start toki manually with: toki start");
    Ok(())
}
