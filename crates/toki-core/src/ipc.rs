use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::{atomic::Ordering, Arc},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{UnixListener, UnixStream},
    sync::Mutex,
};

/// IPC request from CLI to daemon
#[derive(Serialize, Deserialize, Debug)]
pub enum IpcRequest {
    Status,
    Shutdown,
}

/// IPC response from daemon to CLI
#[derive(Serialize, Deserialize, Debug)]
pub enum IpcResponse {
    Status {
        running: bool,
        current_window: Option<String>,
        current_issue: Option<String>,
        session_duration: u64,
    },
    Shutdown,
}

#[derive(Debug)]
pub struct IpcClient {
    sock_path: PathBuf,
}

impl IpcClient {
    #[must_use]
    pub fn new(sock_path: &Path) -> Self {
        Self {
            sock_path: sock_path.to_path_buf(),
        }
    }

    pub async fn send_command(&self, request: IpcRequest) -> Result<IpcResponse> {
        let mut stream = UnixStream::connect(&self.sock_path).await?;

        let encoded = bincode::serialize(&request)?;
        stream.write_all(&encoded).await?;
        stream.shutdown().await?;

        let mut buffer = Vec::new();
        stream.read_to_end(&mut buffer).await?;
        let response: IpcResponse = bincode::deserialize(&buffer)?;

        Ok(response)
    }
}

pub struct DaemonIpcHandler {
    current_window: Arc<Mutex<Option<String>>>,
    current_issue: Arc<Mutex<Option<String>>>,
    session_start: Arc<Mutex<chrono::DateTime<chrono::Utc>>>,
    shutdown_signal: Arc<std::sync::atomic::AtomicBool>,
}

impl DaemonIpcHandler {
    pub fn new(shutdown_signal: Arc<std::sync::atomic::AtomicBool>) -> Self {
        Self {
            current_window: Arc::new(Mutex::new(None)),
            current_issue: Arc::new(Mutex::new(None)),
            session_start: Arc::new(Mutex::new(chrono::Utc::now())),
            shutdown_signal,
        }
    }

    pub async fn set_current_window(&self, window_title: Option<String>) {
        let mut lock = self.current_window.lock().await;
        *lock = window_title;
    }

    pub async fn set_current_issue(&self, issue: Option<String>) {
        let mut lock = self.current_issue.lock().await;
        *lock = issue;
    }

    pub async fn reset_session_start(&self) {
        let mut lock = self.session_start.lock().await;
        *lock = chrono::Utc::now();
    }

    pub async fn handle(
        &self,
        stream: &mut UnixStream,
        request: IpcRequest,
    ) -> Result<(), anyhow::Error> {
        let response = match request {
            IpcRequest::Status => {
                let window = self.current_window.lock().await;
                let issue = self.current_issue.lock().await;
                let start = self.session_start.lock().await;
                let duration = chrono::Utc::now().signed_duration_since(*start);

                IpcResponse::Status {
                    running: true,
                    current_window: window.clone(),
                    current_issue: issue.clone(),
                    session_duration: duration.num_seconds() as u64,
                }
            }
            IpcRequest::Shutdown => {
                self.shutdown_signal.store(true, Ordering::SeqCst);
                IpcResponse::Shutdown
            }
        };

        let encoded = bincode::serialize(&response)?;
        stream.write_all(&encoded).await?;
        Ok(())
    }
}

pub async fn listen(handler: Arc<DaemonIpcHandler>, sock_path: &Path) -> io::Result<()> {
    if sock_path.exists() {
        fs::remove_file(sock_path)?;
    }
    let listener = UnixListener::bind(sock_path)?;

    loop {
        match listener.accept().await {
            Ok((mut stream, _)) => {
                let handler = handler.clone();
                tokio::spawn(async move {
                    let mut buf = vec![0; 1024];
                    match stream.read(&mut buf).await {
                        Ok(n) if n > 0 => match bincode::deserialize::<IpcRequest>(&buf[..n]) {
                            Ok(request) => {
                                if let Err(e) = handler.handle(&mut stream, request).await {
                                    log::error!("IPC handle error: {e}");
                                }
                            }
                            Err(e) => {
                                log::error!("IPC deserialize error: {e}");
                            }
                        },
                        Ok(_) => {} // Connection closed
                        Err(e) => {
                            log::error!("IPC read error: {e}");
                        }
                    }
                });
            }
            Err(e) => {
                log::error!("IPC accept error: {e}");
            }
        }
    }
}
