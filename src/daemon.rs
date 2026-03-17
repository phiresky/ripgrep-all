use crate::preproc_cache::{CacheKey, PreprocCache, open_cache_db};
use crate::config::RgaConfig;
use anyhow::{Context, Result};
use log::{error, info};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

#[derive(Serialize, Deserialize, Debug)]
pub enum DaemonRequest {
    Get(CacheKey),
    Set(CacheKey, Vec<u8>),
}

#[derive(Serialize, Deserialize, Debug)]
pub enum DaemonResponse {
    Get(Option<Vec<u8>>),
    Set,
    Error(String),
}

pub async fn run_daemon(path: &std::path::Path, port: u16) -> Result<()> {
    let addr = format!("127.0.0.1:{}", port);
    let listener = TcpListener::bind(&addr).await.context("Failed to bind to daemon address")?;
    info!("rga daemon listening on {}", addr);

    let mut config = RgaConfig::default();
    config.cache.path = crate::config::CachePath(path.to_string_lossy().to_string());
    let cache = open_cache_db(&config).await?;
    let cache = std::sync::Arc::new(tokio::sync::Mutex::new(cache));

    loop {
        let (socket, _) = listener.accept().await?;
        let cache = cache.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_connection(socket, cache).await {
                error!("Error handling connection: {}", e);
            }
        });
    }
}

async fn handle_connection(mut socket: TcpStream, cache: std::sync::Arc<tokio::sync::Mutex<Box<dyn PreprocCache + Send>>>) -> Result<()> {
    let (reader, mut writer) = socket.split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            break;
        }

        let request: DaemonRequest = match serde_json::from_str(&line) {
            Ok(req) => req,
            Err(e) => {
                let resp = DaemonResponse::Error(format!("Invalid request: {}", e));
                let resp_json = serde_json::to_string(&resp)? + "\n";
                writer.write_all(resp_json.as_bytes()).await?;
                continue;
            }
        };

        let response = match request {
            DaemonRequest::Get(key) => {
                let cache = cache.lock().await;
                match cache.get(&key).await {
                    Ok(val) => DaemonResponse::Get(val),
                    Err(e) => DaemonResponse::Error(e.to_string()),
                }
            }
            DaemonRequest::Set(key, value) => {
                let mut cache = cache.lock().await;
                match cache.set(&key, value).await {
                    Ok(_) => DaemonResponse::Set,
                    Err(e) => DaemonResponse::Error(e.to_string()),
                }
            }
        };

        let resp_json = serde_json::to_string(&response)? + "\n";
        writer.write_all(resp_json.as_bytes()).await?;
    }

    Ok(())
}

pub struct DaemonCacheClient {
    port: u16,
}

impl DaemonCacheClient {
    pub fn new(port: u16) -> Self {
        Self { port }
    }

    async fn connect(&self) -> Result<TcpStream> {
        TcpStream::connect(format!("127.0.0.1:{}", self.port)).await.context("Failed to connect to rga daemon")
    }
}

#[async_trait::async_trait]
impl PreprocCache for DaemonCacheClient {
    async fn get(&self, key: &CacheKey) -> Result<Option<Vec<u8>>> {
        let mut stream = self.connect().await?;
        let req = DaemonRequest::Get(key.clone());
        let req_json = serde_json::to_string(&req)? + "\n";
        stream.write_all(req_json.as_bytes()).await?;

        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).await?;
        
        match serde_json::from_str(&line)? {
            DaemonResponse::Get(val) => Ok(val),
            DaemonResponse::Error(e) => Err(anyhow::anyhow!("Daemon error: {}", e)),
            _ => Err(anyhow::anyhow!("Unexpected response from daemon")),
        }
    }

    async fn set(&mut self, key: &CacheKey, value: Vec<u8>) -> Result<()> {
        let mut stream = self.connect().await?;
        let req = DaemonRequest::Set(key.clone(), value);
        let req_json = serde_json::to_string(&req)? + "\n";
        stream.write_all(req_json.as_bytes()).await?;

        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).await?;
        
        match serde_json::from_str(&line)? {
            DaemonResponse::Set => Ok(()),
            DaemonResponse::Error(e) => Err(anyhow::anyhow!("Daemon error: {}", e)),
            _ => Err(anyhow::anyhow!("Unexpected response from daemon")),
        }
    }
}
