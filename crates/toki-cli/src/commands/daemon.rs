/// Daemon lifecycle management commands
use anyhow::Result;
use std::{env, fs, io, path::Path, process::Command, thread::sleep, time};
use sysinfo::{Pid, System};
use toki_core::{
    config::get_data_dir,
    ipc::{IpcClient, IpcRequest, IpcResponse},
    Daemon,
};
use toki_storage::Database;

pub fn start_daemon(data_dir: &Path) -> Result<()> {
    let pid_file_path = data_dir.join("toki.pid");
    let sock_path = data_dir.join("toki.sock");

    // 1. Check if daemon is already running
    if pid_file_path.exists() {
        if let Ok(pid_str) = fs::read_to_string(&pid_file_path) {
            if let Ok(pid) = pid_str.trim().parse::<usize>() {
                let mut sys = System::new();
                if sys.refresh_process(Pid::from(pid)) {
                    log::info!("Daemon is already running (PID: {pid}).");
                    return Ok(());
                }
            }
        }
        // If pid file is stale, remove it
        log::warn!("Removing stale PID file.");
        let _ = fs::remove_file(&pid_file_path);
    }

    // 2. Clean up old socket if it exists
    if sock_path.exists() {
        log::warn!("Removing stale socket file.");
        fs::remove_file(&sock_path)?;
    }

    log::info!("Starting Toki daemon...");

    // 3. Spawn a new process for the daemon
    let current_exe = env::current_exe()?;
    let current_dir = env::current_dir()?;
    let child = Command::new(current_exe)
        .arg("daemon-internal-start")
        .current_dir(current_dir) // Explicitly set the working directory for the daemon
        .spawn()?;

    // 4. In parent process, write PID and exit
    log::info!("Daemon process started with PID: {}", child.id());
    fs::write(&pid_file_path, child.id().to_string())?;

    Ok(())
}

pub async fn run_daemon_process() -> Result<()> {
    // This is the detached daemon process
    // We must set up logging here, as this is a new process.
    if let Err(e) = setup_daemon_logging() {
        // If logging fails, we have no way to report errors. Panicking is the only option.
        panic!("Failed to set up daemon logging: {e}");
    }
    log::info!("Daemon process started internally.");

    if let Err(e) = daemon_main_logic().await {
        log::error!("Daemon main logic exited with a fatal error: {e:#}");
        return Err(e);
    }

    Ok(())
}

async fn daemon_main_logic() -> Result<()> {
    let db = Database::new(None)?;
    let mut daemon = Daemon::new(db, 10)?;
    daemon.run_with_signals().await
}

pub async fn stop_daemon(data_dir: &Path) -> Result<()> {
    let pid_file_path = data_dir.join("toki.pid");
    let sock_path = data_dir.join("toki.sock");

    if !pid_file_path.exists() {
        log::info!("Daemon is not running (no PID file).");
        // Also remove socket if it exists for consistency
        if sock_path.exists() {
            fs::remove_file(&sock_path)?;
        }
        return Ok(());
    }

    let pid_str = fs::read_to_string(&pid_file_path)?;
    let pid = pid_str
        .trim()
        .parse::<usize>()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    log::info!("Stopping Toki daemon (PID: {pid})...");
    let client = IpcClient::new(&sock_path);

    match client.send_command(IpcRequest::Shutdown).await {
        Ok(IpcResponse::Shutdown) => {
            log::info!("Daemon shutdown signal sent. Waiting for process to exit...");
            sleep(time::Duration::from_secs(2));

            let mut sys = System::new();
            if sys.refresh_process(Pid::from(pid)) {
                log::warn!("Daemon did not stop gracefully. Force killing...");
                if let Some(process) = sys.process(Pid::from(pid)) {
                    process.kill();
                }
            } else {
                log::info!("Daemon stopped successfully.");
            }
        }
        Ok(resp) => log::error!("Received unexpected response from daemon: {resp:?}"),
        Err(e) => {
            log::error!("Failed to send shutdown command: {e}. Forcing cleanup.");
            let mut sys = System::new();
            if sys.refresh_process(Pid::from(pid)) {
                if let Some(process) = sys.process(Pid::from(pid)) {
                    process.kill();
                    log::info!("Process killed.");
                }
            }
        }
    }

    // Cleanup
    fs::remove_file(&pid_file_path)?;
    if sock_path.exists() {
        fs::remove_file(&sock_path)?;
    }

    Ok(())
}

pub async fn show_status(data_dir: &Path) -> Result<()> {
    let sock_path = data_dir.join("toki.sock");

    if !sock_path.exists() {
        println!("Daemon Status: Not running");
        return Ok(());
    }

    let client = IpcClient::new(&sock_path);
    match client.send_command(IpcRequest::Status).await {
        Ok(IpcResponse::Status {
            running,
            current_window,
            current_issue,
            session_duration,
        }) => {
            println!(
                "Daemon Status: {}",
                if running { "Running" } else { "Stopped" }
            );
            println!("\nCurrent Activity:");
            println!(
                "  Window: {}",
                current_window.unwrap_or_else(|| "None".to_string())
            );
            println!(
                "  Issue: {}",
                current_issue.unwrap_or_else(|| "None".to_string())
            );

            let hours = session_duration / 3600;
            let minutes = (session_duration % 3600) / 60;
            let seconds = session_duration % 60;
            println!("\nSession Duration: {hours:02}:{minutes:02}:{seconds:02}");
        }
        Ok(_) => anyhow::bail!("Unexpected response from daemon"),
        Err(e) => {
            log::error!("Failed to get status: {e}");
            println!("Daemon Status: Not running (or not responding)");
        }
    }
    Ok(())
}

fn setup_daemon_logging() -> Result<()> {
    use std::fs::{create_dir_all, OpenOptions};

    let log_path = get_data_dir()?.join("toki.log");

    if let Some(parent) = log_path.parent() {
        create_dir_all(parent)?;
    }

    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;

    env_logger::Builder::from_default_env()
        .target(env_logger::Target::Pipe(Box::new(log_file)))
        .filter_level(log::LevelFilter::Debug)
        .init();

    Ok(())
}
