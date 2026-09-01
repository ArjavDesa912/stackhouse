//! # Dependency Vulnerability Scanning & CVE Alerting
//!
//! Scans project dependencies for known vulnerabilities, monitors
//! CVE databases, and sends alerts when new vulnerabilities are found.

use crate::db::{SqlValue, StackhouseStore};
use crate::error::StackhouseResult;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vulnerability {
    pub id: String,
    pub cve_id: String,
    pub package_name: String,
    pub package_version: String,
    pub severity: VulnSeverity,
    pub cvss_score: f64,
    pub title: String,
    pub description: String,
    pub fixed_version: Option<String>,
    pub references: Vec<String>,
    pub published_at: String,
    pub discovered_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VulnSeverity {
    Critical,
    High,
    Medium,
    Low,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    pub scan_id: String,
    pub tenant_id: i64,
    pub project_name: String,
    pub total_dependencies: u32,
    pub vulnerabilities_found: u32,
    pub critical_count: u32,
    pub high_count: u32,
    pub medium_count: u32,
    pub low_count: u32,
    pub vulnerabilities: Vec<Vulnerability>,
    pub scanned_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyEntry {
    pub name: String,
    pub version: String,
    pub ecosystem: String, // "cargo", "npm", "pip", etc.
}

#[derive(Clone)]
pub struct VulnScanService {
    store: Arc<StackhouseStore>,
}

impl VulnScanService {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let service = Self { store };
        service.initialize_tables().await?;
        info!("🔍 Vulnerability scanning service initialized");
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS stackhouse_vuln_scans (
                scan_id TEXT PRIMARY KEY,
                tenant_id BIGINT NOT NULL,
                project_name TEXT NOT NULL,
                total_deps INTEGER DEFAULT 0,
                vuln_count INTEGER DEFAULT 0,
                critical_count INTEGER DEFAULT 0,
                high_count INTEGER DEFAULT 0,
                results JSONB DEFAULT '[]',
                scanned_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE TABLE IF NOT EXISTS stackhouse_vuln_advisories (
                cve_id TEXT PRIMARY KEY,
                package_name TEXT NOT NULL,
                ecosystem TEXT NOT NULL,
                severity TEXT NOT NULL,
                cvss_score FLOAT DEFAULT 0,
                affected_versions TEXT NOT NULL,
                fixed_version TEXT,
                title TEXT NOT NULL,
                description TEXT,
                published_at TIMESTAMPTZ
            );
            CREATE TABLE IF NOT EXISTS stackhouse_vuln_alerts (
                id BIGSERIAL PRIMARY KEY,
                tenant_id BIGINT NOT NULL,
                cve_id TEXT NOT NULL,
                package_name TEXT NOT NULL,
                severity TEXT NOT NULL,
                acknowledged BOOLEAN DEFAULT FALSE,
                created_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE INDEX IF NOT EXISTS idx_vuln_scans_tenant ON stackhouse_vuln_scans(tenant_id);
            CREATE INDEX IF NOT EXISTS idx_vuln_advisories_pkg ON stackhouse_vuln_advisories(package_name, ecosystem);
            CREATE INDEX IF NOT EXISTS idx_vuln_alerts_tenant ON stackhouse_vuln_alerts(tenant_id, acknowledged);
        "#.to_string()).await?;
        Ok(())
    }

    /// Scan dependencies against known vulnerability database
    pub async fn scan_dependencies(
        &self,
        tenant_id: i64,
        project_name: &str,
        deps: &[DependencyEntry],
    ) -> StackhouseResult<ScanResult> {
        let scan_id = uuid::Uuid::new_v4().to_string();
        let mut vulnerabilities = Vec::new();

        for dep in deps {
            let vulns = self
                .check_package(&dep.name, &dep.version, &dep.ecosystem)
                .await?;
            vulnerabilities.extend(vulns);
        }

        let critical_count = vulnerabilities
            .iter()
            .filter(|v| matches!(v.severity, VulnSeverity::Critical))
            .count() as u32;
        let high_count = vulnerabilities
            .iter()
            .filter(|v| matches!(v.severity, VulnSeverity::High))
            .count() as u32;
        let medium_count = vulnerabilities
            .iter()
            .filter(|v| matches!(v.severity, VulnSeverity::Medium))
            .count() as u32;
        let low_count = vulnerabilities
            .iter()
            .filter(|v| matches!(v.severity, VulnSeverity::Low))
            .count() as u32;

        let result = ScanResult {
            scan_id: scan_id.clone(),
            tenant_id,
            project_name: project_name.to_string(),
            total_dependencies: deps.len() as u32,
            vulnerabilities_found: vulnerabilities.len() as u32,
            critical_count,
            high_count,
            medium_count,
            low_count,
            vulnerabilities: vulnerabilities.clone(),
            scanned_at: chrono::Utc::now().to_rfc3339(),
        };

        // Persist scan result
        self.store.execute(
            "INSERT INTO stackhouse_vuln_scans (scan_id, tenant_id, project_name, total_deps, vuln_count, critical_count, high_count, results) VALUES (?, ?, ?, ?, ?, ?, ?, ?::jsonb)".to_string(),
            vec![
                SqlValue::Text(scan_id),
                SqlValue::Integer(tenant_id),
                SqlValue::Text(project_name.to_string()),
                SqlValue::Integer(deps.len() as i64),
                SqlValue::Integer(vulnerabilities.len() as i64),
                SqlValue::Integer(critical_count as i64),
                SqlValue::Integer(high_count as i64),
                SqlValue::Text(serde_json::to_string(&vulnerabilities).unwrap_or_default()),
            ],
        ).await?;

        // Create alerts for critical/high vulnerabilities
        for vuln in &vulnerabilities {
            if matches!(vuln.severity, VulnSeverity::Critical | VulnSeverity::High) {
                self.create_alert(tenant_id, &vuln.cve_id, &vuln.package_name, &vuln.severity)
                    .await?;
            }
        }

        Ok(result)
    }

    async fn check_package(
        &self,
        name: &str,
        version: &str,
        ecosystem: &str,
    ) -> StackhouseResult<Vec<Vulnerability>> {
        // Query local advisory database
        let rows = self.store.query(
            "SELECT cve_id, severity, cvss_score, affected_versions, fixed_version, title, description, published_at FROM stackhouse_vuln_advisories WHERE package_name = ? AND ecosystem = ?".to_string(),
            vec![SqlValue::Text(name.to_string()), SqlValue::Text(ecosystem.to_string())],
        ).await?;

        let mut vulns = Vec::new();
        for row in rows {
            let affected = row
                .iter()
                .find(|(k, _)| k == "affected_versions")
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("");
            // Simple version range check (in production, use semver crate)
            if self.version_affected(version, affected) {
                let severity_str = row
                    .iter()
                    .find(|(k, _)| k == "severity")
                    .and_then(|(_, v)| v.as_str())
                    .unwrap_or("medium");
                let severity = match severity_str {
                    "critical" => VulnSeverity::Critical,
                    "high" => VulnSeverity::High,
                    "low" => VulnSeverity::Low,
                    _ => VulnSeverity::Medium,
                };

                vulns.push(Vulnerability {
                    id: uuid::Uuid::new_v4().to_string(),
                    cve_id: row
                        .iter()
                        .find(|(k, _)| k == "cve_id")
                        .and_then(|(_, v)| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    package_name: name.to_string(),
                    package_version: version.to_string(),
                    severity,
                    cvss_score: row
                        .iter()
                        .find(|(k, _)| k == "cvss_score")
                        .and_then(|(_, v)| v.as_f64())
                        .unwrap_or(0.0),
                    title: row
                        .iter()
                        .find(|(k, _)| k == "title")
                        .and_then(|(_, v)| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    description: row
                        .iter()
                        .find(|(k, _)| k == "description")
                        .and_then(|(_, v)| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    fixed_version: row
                        .iter()
                        .find(|(k, _)| k == "fixed_version")
                        .and_then(|(_, v)| v.as_str())
                        .map(|s| s.to_string()),
                    references: vec![],
                    published_at: row
                        .iter()
                        .find(|(k, _)| k == "published_at")
                        .and_then(|(_, v)| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    discovered_at: chrono::Utc::now().to_rfc3339(),
                });
            }
        }
        Ok(vulns)
    }

    fn version_affected(&self, current: &str, affected_range: &str) -> bool {
        // Simple check: "< X.Y.Z" means all versions below are affected
        if affected_range.starts_with("< ") {
            let threshold = &affected_range[2..];
            return current < threshold;
        }
        affected_range.contains(current)
    }

    async fn create_alert(
        &self,
        tenant_id: i64,
        cve_id: &str,
        package: &str,
        severity: &VulnSeverity,
    ) -> StackhouseResult<()> {
        let sev_str = serde_json::to_string(severity)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string();
        self.store.execute(
            "INSERT INTO stackhouse_vuln_alerts (tenant_id, cve_id, package_name, severity) VALUES (?, ?, ?, ?)".to_string(),
            vec![
                SqlValue::Integer(tenant_id),
                SqlValue::Text(cve_id.to_string()),
                SqlValue::Text(package.to_string()),
                SqlValue::Text(sev_str),
            ],
        ).await?;
        Ok(())
    }

    /// Get unacknowledged alerts
    pub async fn get_alerts(&self, tenant_id: i64) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            "SELECT id, cve_id, package_name, severity, created_at FROM stackhouse_vuln_alerts WHERE tenant_id = ? AND acknowledged = false ORDER BY created_at DESC".to_string(),
            vec![SqlValue::Integer(tenant_id)],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.into_iter().collect::<HashMap<_, _>>()))
            .collect())
    }

    /// Acknowledge an alert
    pub async fn acknowledge_alert(&self, alert_id: i64) -> StackhouseResult<()> {
        self.store
            .execute(
                "UPDATE stackhouse_vuln_alerts SET acknowledged = true WHERE id = ?".to_string(),
                vec![SqlValue::Integer(alert_id)],
            )
            .await?;
        Ok(())
    }
}
