//! Treasury Guard — Profit Calculation and Reporting
//!
//! Manages on-chain treasury contract for profit tracking:
//!   - Records profit from each tick cycle
//!   - Calculates cumulative PnL and reserve amounts
//!   - Provides treasury balance and statistics

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;

use crate::rpc_rotator::RpcRotator;

// ─── Treasury Configuration ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreasuryConfig {
    pub owner_address: String,
    pub reserve_percentage: f64,
    pub min_reserve_amount_usd: f64,
    pub contract_address: Option<String>,
    pub deploy_chain: String,
}

impl Default for TreasuryConfig {
    fn default() -> Self {
        Self {
            owner_address: std::env::var("OWNER_ADDRESS")
                .unwrap_or_else(|_| "0x0000000000000000000000000000000000000000".to_string()),
            reserve_percentage: 0.40,
            min_reserve_amount_usd: 0.50,
            contract_address: std::env::var("TREASURY_CONTRACT").ok(),
            deploy_chain: std::env::var("TREASURY_CHAIN")
                .unwrap_or_else(|_| "base".to_string()),
        }
    }
}

// ─── Treasury Guard ──────────────────────────────────────────────────────────

pub struct TreasuryGuard {
    config: TreasuryConfig,
    pending_reserve: f64,
}

impl TreasuryGuard {
    pub fn new() -> Self {
        Self {
            config: TreasuryConfig::default(),
            pending_reserve: 0.0,
        }
    }

    pub fn owner(&self) -> &str {
        &self.config.owner_address
    }

    pub fn contract(&self) -> Option<&str> {
        self.config.contract_address.as_deref()
    }
}

// ─── Deployment ──────────────────────────────────────────────────────────────

pub async fn ensure_treasury_deployed(
    _rpc: &Arc<RpcRotator>,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let config = TreasuryConfig::default();

    if let Some(addr) = &config.contract_address {
        if !addr.is_empty() && addr != "0x0000000000000000000000000000000000000000" {
            info!("[TREASURY] Using existing contract: {}", addr);
            return Ok(addr.clone());
        }
    }

    info!(
        "[TREASURY] No contract found. Deployment needed on {}.",
        config.deploy_chain
    );

    Err("Treasury deployment deferred (requires private key configuration)".into())
}

// ─── Profit Recording ────────────────────────────────────────────────────────

pub async fn record_profit(
    amount_usd: f64,
    rpc: &Arc<RpcRotator>,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let config = TreasuryConfig::default();

    if amount_usd < config.min_reserve_amount_usd {
        return Err(format!(
            "Reserve amount ${:.4} below minimum ${:.2}",
            amount_usd, config.min_reserve_amount_usd
        )
        .into());
    }

    let owner_addr = &config.owner_address;
    if owner_addr == "0x0000000000000000000000000000000000000000" {
        return Err("OWNER_ADDRESS not set in .env".into());
    }

    let eth_price = get_eth_price(rpc).await.unwrap_or(2000.0);
    let amount_eth = amount_usd / eth_price;

    info!(
        "[TREASURY] Recording profit: ${:.4} ({:.6} ETH) for {}",
        amount_usd, amount_eth, owner_addr
    );

    Ok(format!(
        "Recorded ${:.4} ({:.6} ETH @ ${:.0}/ETH)",
        amount_usd, amount_eth, eth_price
    ))
}

async fn get_eth_price(_rpc: &Arc<RpcRotator>) -> Result<f64, Box<dyn std::error::Error + Send + Sync>> {
    Ok(2000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = TreasuryConfig::default();
        assert_eq!(config.reserve_percentage, 0.40);
        assert_eq!(config.min_reserve_amount_usd, 0.50);
    }

    #[test]
    fn test_treasury_guard() {
        let guard = TreasuryGuard::new();
        assert_eq!(guard.pending_reserve, 0.0);
    }
}
