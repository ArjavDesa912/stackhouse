//! # Invoice Generation & Tax Calculation
//!
//! Generates PDF-ready invoices, calculates taxes via external providers
//! (Avalara, TaxJar), and manages invoice lifecycle (draft, sent, paid, void).

use crate::db::{SqlValue, StackhouseStore};
use crate::error::{StackhouseError, StackhouseResult};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Invoice {
    pub id: String,
    pub tenant_id: i64,
    pub invoice_number: String,
    pub status: InvoiceStatus,
    pub currency: String,
    pub subtotal_cents: i64,
    pub tax_cents: i64,
    pub total_cents: i64,
    pub line_items: Vec<InvoiceLineItem>,
    pub tax_breakdown: Vec<TaxLine>,
    pub billing_address: BillingAddress,
    pub due_date: String,
    pub paid_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InvoiceStatus {
    Draft,
    Sent,
    Paid,
    Overdue,
    Void,
    Refunded,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvoiceLineItem {
    pub description: String,
    pub quantity: f64,
    pub unit_price_cents: i64,
    pub amount_cents: i64,
    pub tax_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxLine {
    pub jurisdiction: String,
    pub tax_type: String,
    pub rate: f64,
    pub amount_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BillingAddress {
    pub company_name: Option<String>,
    pub line1: String,
    pub line2: Option<String>,
    pub city: String,
    pub state: Option<String>,
    pub postal_code: String,
    pub country: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxCalculationRequest {
    pub from_address: BillingAddress,
    pub to_address: BillingAddress,
    pub line_items: Vec<InvoiceLineItem>,
    pub currency: String,
}

#[derive(Clone)]
pub struct InvoiceService {
    store: Arc<StackhouseStore>,
    tax_provider: TaxProvider,
}

#[derive(Clone)]
enum TaxProvider {
    Avalara { api_key: String, sandbox: bool },
    TaxJar { api_key: String },
    None,
}

impl InvoiceService {
    pub async fn new(store: Arc<StackhouseStore>) -> StackhouseResult<Self> {
        let tax_provider = if let Ok(key) = std::env::var("AVALARA_API_KEY") {
            TaxProvider::Avalara {
                api_key: key,
                sandbox: std::env::var("AVALARA_SANDBOX").is_ok(),
            }
        } else if let Ok(key) = std::env::var("TAXJAR_API_KEY") {
            TaxProvider::TaxJar { api_key: key }
        } else {
            TaxProvider::None
        };

        let service = Self {
            store,
            tax_provider,
        };
        service.initialize_tables().await?;
        info!("🧾 Invoice service initialized");
        Ok(service)
    }

    async fn initialize_tables(&self) -> StackhouseResult<()> {
        self.store
            .execute_batch(
                r#"
            CREATE TABLE IF NOT EXISTS stackhouse_invoices (
                id TEXT PRIMARY KEY,
                tenant_id BIGINT NOT NULL,
                invoice_number TEXT NOT NULL UNIQUE,
                status TEXT DEFAULT 'draft',
                currency TEXT DEFAULT 'usd',
                subtotal_cents BIGINT DEFAULT 0,
                tax_cents BIGINT DEFAULT 0,
                total_cents BIGINT DEFAULT 0,
                line_items JSONB DEFAULT '[]',
                tax_breakdown JSONB DEFAULT '[]',
                billing_address JSONB DEFAULT '{}',
                due_date TIMESTAMPTZ,
                paid_at TIMESTAMPTZ,
                created_at TIMESTAMPTZ DEFAULT NOW()
            );
            CREATE TABLE IF NOT EXISTS stackhouse_invoice_sequences (
                tenant_id BIGINT PRIMARY KEY,
                next_number BIGINT DEFAULT 1
            );
            CREATE INDEX IF NOT EXISTS idx_invoices_tenant ON stackhouse_invoices(tenant_id);
            CREATE INDEX IF NOT EXISTS idx_invoices_status ON stackhouse_invoices(status);
        "#
                .to_string(),
            )
            .await?;
        Ok(())
    }

    /// Generate an invoice
    pub async fn create_invoice(
        &self,
        tenant_id: i64,
        line_items: Vec<InvoiceLineItem>,
        address: BillingAddress,
        currency: &str,
        due_days: u32,
    ) -> StackhouseResult<Invoice> {
        let id = uuid::Uuid::new_v4().to_string();
        let invoice_number = self.next_invoice_number(tenant_id).await?;

        let subtotal: i64 = line_items.iter().map(|li| li.amount_cents).sum();

        // Calculate tax
        let tax_breakdown = self.calculate_tax(&line_items, &address, currency).await?;
        let tax_total: i64 = tax_breakdown.iter().map(|t| t.amount_cents).sum();

        let total = subtotal + tax_total;
        let due_date = (chrono::Utc::now() + chrono::Duration::days(due_days as i64)).to_rfc3339();

        self.store.execute(
            "INSERT INTO stackhouse_invoices (id, tenant_id, invoice_number, currency, subtotal_cents, tax_cents, total_cents, line_items, tax_breakdown, billing_address, due_date) VALUES (?, ?, ?, ?, ?, ?, ?, ?::jsonb, ?::jsonb, ?::jsonb, ?::timestamptz)".to_string(),
            vec![
                SqlValue::Text(id.clone()),
                SqlValue::Integer(tenant_id),
                SqlValue::Text(invoice_number.clone()),
                SqlValue::Text(currency.to_string()),
                SqlValue::Integer(subtotal),
                SqlValue::Integer(tax_total),
                SqlValue::Integer(total),
                SqlValue::Text(serde_json::to_string(&line_items).unwrap_or_default()),
                SqlValue::Text(serde_json::to_string(&tax_breakdown).unwrap_or_default()),
                SqlValue::Text(serde_json::to_string(&address).unwrap_or_default()),
                SqlValue::Text(due_date.clone()),
            ],
        ).await?;

        Ok(Invoice {
            id,
            tenant_id,
            invoice_number,
            status: InvoiceStatus::Draft,
            currency: currency.to_string(),
            subtotal_cents: subtotal,
            tax_cents: tax_total,
            total_cents: total,
            line_items,
            tax_breakdown,
            billing_address: address,
            due_date,
            paid_at: None,
            created_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    /// Calculate tax using configured provider
    async fn calculate_tax(
        &self,
        items: &[InvoiceLineItem],
        address: &BillingAddress,
        _currency: &str,
    ) -> StackhouseResult<Vec<TaxLine>> {
        match &self.tax_provider {
            TaxProvider::Avalara { api_key, sandbox } => {
                self.calculate_tax_avalara(api_key, *sandbox, items, address)
                    .await
            }
            TaxProvider::TaxJar { api_key } => {
                self.calculate_tax_taxjar(api_key, items, address).await
            }
            TaxProvider::None => {
                // No tax provider — return empty
                Ok(vec![])
            }
        }
    }

    async fn calculate_tax_avalara(
        &self,
        api_key: &str,
        sandbox: bool,
        items: &[InvoiceLineItem],
        address: &BillingAddress,
    ) -> StackhouseResult<Vec<TaxLine>> {
        let base_url = if sandbox {
            "https://sandbox-rest.avatax.com"
        } else {
            "https://rest.avatax.com"
        };
        let client = reqwest::Client::new();

        let lines: Vec<Value> = items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                json!({
                    "number": i + 1,
                    "amount": item.amount_cents as f64 / 100.0,
                    "taxCode": item.tax_code.as_deref().unwrap_or("SW054000"),
                })
            })
            .collect();

        let body = json!({
            "type": "SalesInvoice",
            "companyCode": "DEFAULT",
            "addresses": {
                "shipTo": {
                    "city": address.city,
                    "region": address.state,
                    "postalCode": address.postal_code,
                    "country": address.country,
                }
            },
            "lines": lines,
            "commit": false,
        });

        let resp = client
            .post(format!("{}/api/v2/transactions/create", base_url))
            .header("Authorization", format!("Basic {}", api_key))
            .json(&body)
            .send()
            .await
            .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("Avalara: {}", e)))?;

        let data: Value = resp.json().await.unwrap_or(json!({}));

        let tax_lines: Vec<TaxLine> = data["summary"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .map(|s| TaxLine {
                jurisdiction: s["jurisName"].as_str().unwrap_or("").to_string(),
                tax_type: s["taxName"].as_str().unwrap_or("Sales Tax").to_string(),
                rate: s["rate"].as_f64().unwrap_or(0.0),
                amount_cents: (s["tax"].as_f64().unwrap_or(0.0) * 100.0) as i64,
            })
            .collect();

        Ok(tax_lines)
    }

    async fn calculate_tax_taxjar(
        &self,
        api_key: &str,
        items: &[InvoiceLineItem],
        address: &BillingAddress,
    ) -> StackhouseResult<Vec<TaxLine>> {
        let client = reqwest::Client::new();
        let amount: f64 = items.iter().map(|i| i.amount_cents as f64 / 100.0).sum();

        let body = json!({
            "to_country": address.country,
            "to_state": address.state,
            "to_zip": address.postal_code,
            "amount": amount,
            "shipping": 0,
        });

        let resp = client
            .post("https://api.taxjar.com/v2/taxes")
            .bearer_auth(api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| StackhouseError::Internal(anyhow::anyhow!("TaxJar: {}", e)))?;

        let data: Value = resp.json().await.unwrap_or(json!({}));
        let tax_amount = data["tax"]["amount_to_collect"].as_f64().unwrap_or(0.0);
        let rate = data["tax"]["rate"].as_f64().unwrap_or(0.0);

        Ok(vec![TaxLine {
            jurisdiction: address.state.clone().unwrap_or_default(),
            tax_type: "Sales Tax".into(),
            rate,
            amount_cents: (tax_amount * 100.0) as i64,
        }])
    }

    async fn next_invoice_number(&self, tenant_id: i64) -> StackhouseResult<String> {
        self.store.execute(
            "INSERT INTO stackhouse_invoice_sequences (tenant_id, next_number) VALUES (?, 1) ON CONFLICT (tenant_id) DO UPDATE SET next_number = stackhouse_invoice_sequences.next_number + 1".to_string(),
            vec![SqlValue::Integer(tenant_id)],
        ).await?;

        let rows = self
            .store
            .query(
                "SELECT next_number FROM stackhouse_invoice_sequences WHERE tenant_id = ?"
                    .to_string(),
                vec![SqlValue::Integer(tenant_id)],
            )
            .await?;

        let num = rows
            .first()
            .and_then(|r| r.iter().find(|(k, _)| k == "next_number"))
            .and_then(|(_, v)| v.as_i64())
            .unwrap_or(1);
        Ok(format!("INV-{:06}", num))
    }

    /// Mark invoice as paid
    pub async fn mark_paid(&self, invoice_id: &str) -> StackhouseResult<()> {
        self.store
            .execute(
                "UPDATE stackhouse_invoices SET status = 'paid', paid_at = NOW() WHERE id = ?"
                    .to_string(),
                vec![SqlValue::Text(invoice_id.to_string())],
            )
            .await?;
        Ok(())
    }

    /// List invoices for a tenant
    pub async fn list_invoices(&self, tenant_id: i64) -> StackhouseResult<Vec<Value>> {
        let rows = self.store.query(
            "SELECT id, invoice_number, status, currency, total_cents, due_date, paid_at, created_at FROM stackhouse_invoices WHERE tenant_id = ? ORDER BY created_at DESC".to_string(),
            vec![SqlValue::Integer(tenant_id)],
        ).await?;
        Ok(rows
            .into_iter()
            .map(|r| json!(r.into_iter().collect::<HashMap<_, _>>()))
            .collect())
    }
}
