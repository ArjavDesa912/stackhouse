//! # File Scanning (Antivirus + NSFW Detection)
//!
//! Pluggable scanner interface with ClamAV integration and AI-based NSFW detection.

use crate::error::StackhouseResult;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::warn;

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    pub file_path: String,
    pub file_size: u64,
    pub scan_type: ScanType,
    pub status: ScanStatus,
    pub threats: Vec<ThreatInfo>,
    pub scanned_at: String,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanType {
    Antivirus,
    Nsfw,
    Malware,
    Full,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanStatus {
    Clean,
    Infected,
    Suspicious,
    Error,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreatInfo {
    pub threat_type: String,
    pub name: String,
    pub severity: ThreatSeverity,
    pub details: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThreatSeverity {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanConfig {
    pub enabled: bool,
    pub max_file_size_bytes: u64,
    pub clamav_host: Option<String>,
    pub clamav_port: u16,
    pub nsfw_enabled: bool,
    pub nsfw_threshold: f64,
    pub scan_on_upload: bool,
    pub quarantine_infected: bool,
}

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_file_size_bytes: 100 * 1024 * 1024, // 100MB
            clamav_host: std::env::var("CLAMAV_HOST").ok(),
            clamav_port: std::env::var("CLAMAV_PORT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(3310),
            nsfw_enabled: std::env::var("STACKHOUSE_NSFW_SCAN")
                .ok()
                .map(|v| v == "true")
                .unwrap_or(false),
            nsfw_threshold: 0.8,
            scan_on_upload: true,
            quarantine_infected: true,
        }
    }
}

// ============================================================================
// Scanner Trait
// ============================================================================

#[async_trait::async_trait]
pub trait FileScanner: Send + Sync {
    async fn scan(&self, data: &[u8], filename: &str) -> StackhouseResult<ScanResult>;
    fn scan_type(&self) -> ScanType;
}

// ============================================================================
// ClamAV Scanner
// ============================================================================

pub struct ClamAvScanner {
    host: String,
    port: u16,
}

impl ClamAvScanner {
    pub fn new(host: &str, port: u16) -> Self {
        Self {
            host: host.to_string(),
            port,
        }
    }
}

#[async_trait::async_trait]
impl FileScanner for ClamAvScanner {
    async fn scan(&self, data: &[u8], filename: &str) -> StackhouseResult<ScanResult> {
        let start = std::time::Instant::now();

        // Connect to ClamAV daemon via TCP (clamd protocol)
        let result =
            match tokio::net::TcpStream::connect(format!("{}:{}", self.host, self.port)).await {
                Ok(mut stream) => {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};

                    // Send INSTREAM command
                    stream.write_all(b"zINSTREAM\0").await.ok();

                    // Send data in chunks
                    let chunk_size = data.len() as u32;
                    stream.write_all(&chunk_size.to_be_bytes()).await.ok();
                    stream.write_all(data).await.ok();
                    stream.write_all(&[0u8; 4]).await.ok(); // End of stream

                    // Read response
                    let mut response = vec![0u8; 1024];
                    let n = stream.read(&mut response).await.unwrap_or(0);
                    let response_str = String::from_utf8_lossy(&response[..n]).to_string();

                    if response_str.contains("FOUND") {
                        let threat_name = response_str
                            .split(':')
                            .nth(1)
                            .unwrap_or("Unknown")
                            .trim()
                            .to_string();
                        ScanResult {
                            file_path: filename.to_string(),
                            file_size: data.len() as u64,
                            scan_type: ScanType::Antivirus,
                            status: ScanStatus::Infected,
                            threats: vec![ThreatInfo {
                                threat_type: "virus".to_string(),
                                name: threat_name,
                                severity: ThreatSeverity::Critical,
                                details: Some(response_str),
                            }],
                            scanned_at: chrono::Utc::now().to_rfc3339(),
                            duration_ms: start.elapsed().as_millis() as u64,
                        }
                    } else {
                        ScanResult {
                            file_path: filename.to_string(),
                            file_size: data.len() as u64,
                            scan_type: ScanType::Antivirus,
                            status: ScanStatus::Clean,
                            threats: vec![],
                            scanned_at: chrono::Utc::now().to_rfc3339(),
                            duration_ms: start.elapsed().as_millis() as u64,
                        }
                    }
                }
                Err(e) => {
                    warn!("ClamAV connection failed: {}", e);
                    ScanResult {
                        file_path: filename.to_string(),
                        file_size: data.len() as u64,
                        scan_type: ScanType::Antivirus,
                        status: ScanStatus::Error,
                        threats: vec![],
                        scanned_at: chrono::Utc::now().to_rfc3339(),
                        duration_ms: start.elapsed().as_millis() as u64,
                    }
                }
            };

        Ok(result)
    }

    fn scan_type(&self) -> ScanType {
        ScanType::Antivirus
    }
}

// ============================================================================
// NSFW Scanner (AI-based)
// ============================================================================

pub struct NsfwScanner {
    threshold: f64,
    api_endpoint: Option<String>,
}

impl NsfwScanner {
    pub fn new(threshold: f64) -> Self {
        Self {
            threshold,
            api_endpoint: std::env::var("STACKHOUSE_NSFW_API_ENDPOINT").ok(),
        }
    }
}

#[async_trait::async_trait]
impl FileScanner for NsfwScanner {
    async fn scan(&self, data: &[u8], filename: &str) -> StackhouseResult<ScanResult> {
        let start = std::time::Instant::now();

        // Check if file is an image
        let is_image = filename.ends_with(".jpg")
            || filename.ends_with(".jpeg")
            || filename.ends_with(".png")
            || filename.ends_with(".gif")
            || filename.ends_with(".webp")
            || filename.ends_with(".bmp");

        if !is_image {
            return Ok(ScanResult {
                file_path: filename.to_string(),
                file_size: data.len() as u64,
                scan_type: ScanType::Nsfw,
                status: ScanStatus::Skipped,
                threats: vec![],
                scanned_at: chrono::Utc::now().to_rfc3339(),
                duration_ms: start.elapsed().as_millis() as u64,
            });
        }

        // Call external NSFW classification API if configured
        if let Some(endpoint) = &self.api_endpoint {
            let client = reqwest::Client::new();
            let resp = client
                .post(endpoint)
                .header("Content-Type", "application/octet-stream")
                .body(data.to_vec())
                .send()
                .await;

            if let Ok(response) = resp {
                if let Ok(result) = response.json::<Value>().await {
                    let nsfw_score = result["nsfw_score"].as_f64().unwrap_or(0.0);
                    if nsfw_score > self.threshold {
                        return Ok(ScanResult {
                            file_path: filename.to_string(),
                            file_size: data.len() as u64,
                            scan_type: ScanType::Nsfw,
                            status: ScanStatus::Infected,
                            threats: vec![ThreatInfo {
                                threat_type: "nsfw_content".to_string(),
                                name: format!("NSFW content detected (score: {:.2})", nsfw_score),
                                severity: ThreatSeverity::High,
                                details: Some(format!(
                                    "Score: {:.4}, Threshold: {:.4}",
                                    nsfw_score, self.threshold
                                )),
                            }],
                            scanned_at: chrono::Utc::now().to_rfc3339(),
                            duration_ms: start.elapsed().as_millis() as u64,
                        });
                    }
                }
            }
        }

        Ok(ScanResult {
            file_path: filename.to_string(),
            file_size: data.len() as u64,
            scan_type: ScanType::Nsfw,
            status: ScanStatus::Clean,
            threats: vec![],
            scanned_at: chrono::Utc::now().to_rfc3339(),
            duration_ms: start.elapsed().as_millis() as u64,
        })
    }

    fn scan_type(&self) -> ScanType {
        ScanType::Nsfw
    }
}

// ============================================================================
// Composite Scanner
// ============================================================================

pub struct CompositeScanner {
    scanners: Vec<Box<dyn FileScanner>>,
    config: ScanConfig,
}

impl CompositeScanner {
    pub fn new(config: ScanConfig) -> Self {
        let mut scanners: Vec<Box<dyn FileScanner>> = Vec::new();

        if let Some(ref host) = config.clamav_host {
            scanners.push(Box::new(ClamAvScanner::new(host, config.clamav_port)));
        }

        if config.nsfw_enabled {
            scanners.push(Box::new(NsfwScanner::new(config.nsfw_threshold)));
        }

        Self { scanners, config }
    }

    pub fn from_env() -> Self {
        Self::new(ScanConfig::default())
    }

    /// Scan a file with all configured scanners
    pub async fn scan_file(
        &self,
        data: &[u8],
        filename: &str,
    ) -> StackhouseResult<Vec<ScanResult>> {
        if !self.config.enabled {
            return Ok(vec![]);
        }

        if data.len() as u64 > self.config.max_file_size_bytes {
            return Ok(vec![ScanResult {
                file_path: filename.to_string(),
                file_size: data.len() as u64,
                scan_type: ScanType::Full,
                status: ScanStatus::Skipped,
                threats: vec![],
                scanned_at: chrono::Utc::now().to_rfc3339(),
                duration_ms: 0,
            }]);
        }

        let mut results = Vec::new();
        for scanner in &self.scanners {
            match scanner.scan(data, filename).await {
                Ok(result) => results.push(result),
                Err(e) => {
                    warn!("Scanner error for {}: {}", filename, e);
                }
            }
        }
        Ok(results)
    }

    /// Check if any scan result indicates a threat
    pub fn has_threats(results: &[ScanResult]) -> bool {
        results
            .iter()
            .any(|r| matches!(r.status, ScanStatus::Infected))
    }
}
