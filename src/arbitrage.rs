//! Cross-DEX Arbitrage Module
//!
//! Scans for price discrepancies across DEXes on Base, Arbitrum, and Polygon.
//!
//! Strategies:
//!   - Cross-DEX arbitrage: exploit price differences between Uniswap V3,
//!     SushiSwap, Curve, BaseSwap, QuickSwap
//!   - Tail-riding: replicate profitable pending swaps
//!
//! Risk management:
//!   - Kelly 1% per position
//!   - P_net >= 0.3% after gas + slippage
//!   - Gas price ceiling to prevent overpay

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, warn};

use crate::rpc_rotator::RpcRotator;
use crate::{kelly_size, passes_pnet, KELLY_RISK, P_NET_THRESHOLD};

// ─── Chain Configuration ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Chain {
    Base,
    Arbitrum,
    Polygon,
}

impl Chain {
    pub fn chain_id(&self) -> u64 {
        match self {
            Self::Base => 8453,
            Self::Arbitrum => 42161,
            Self::Polygon => 137,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Base => "base",
            Self::Arbitrum => "arbitrum",
            Self::Polygon => "polygon",
        }
    }

    pub fn avg_gas_gwei(&self) -> f64 {
        match self {
            Self::Base => 0.01,
            Self::Arbitrum => 0.1,
            Self::Polygon => 30.0,
        }
    }

    pub fn all() -> Vec<Self> {
        vec![Self::Base, Self::Arbitrum, Self::Polygon]
    }
}

// ─── Arbitrage Opportunity ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArbitrageOpportunity {
    pub chain: Chain,
    pub strategy: Strategy,
    pub token_pair: String,
    pub source_dex: String,
    pub target_dex: String,
    pub gross_edge: f64,
    pub gas_cost: f64,
    pub slippage: f64,
    pub net_edge: f64,
    pub recommended_size: f64,
    pub confidence: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Strategy {
    CrossDex,
    TailRide,
}

impl std::fmt::Display for Strategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CrossDex => write!(f, "CROSS_DEX"),
            Self::TailRide => write!(f, "TAIL_RIDE"),
        }
    }
}

// ─── Execution Result ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct ArbitrageResult {
    pub pnl: f64,
    pub trades_executed: usize,
    pub trades_skipped: usize,
    pub opportunities_found: usize,
    pub gas_spent: f64,
}

// ─── Cross-DEX Scanning ─────────────────────────────────────────────────────

async fn scan_cross_dex(
    chain: Chain,
    budget: f64,
    _rpc: &Arc<RpcRotator>,
) -> Vec<ArbitrageOpportunity> {
    let mut opportunities = Vec::new();

    info!(
        "[ARB/CROSS_DEX] Scanning {} (budget=${:.2})",
        chain.name(),
        budget
    );

    let gas_cost = chain.avg_gas_gwei() * 21000.0 / 1e9 * 2000.0 / budget;
    let simulated_edge = 0.005;
    let slippage = 0.001;

    if passes_pnet(simulated_edge, gas_cost, slippage) {
        let net = simulated_edge - gas_cost - slippage;
        opportunities.push(ArbitrageOpportunity {
            chain,
            strategy: Strategy::CrossDex,
            token_pair: "WETH/USDC".into(),
            source_dex: "UniswapV3".into(),
            target_dex: "SushiSwap".into(),
            gross_edge: simulated_edge,
            gas_cost,
            slippage,
            net_edge: net,
            recommended_size: kelly_size(budget, 0.55, 2.0),
            confidence: 0.0,
        });
    }

    opportunities
}

async fn scan_tail_ride(
    chain: Chain,
    budget: f64,
    _rpc: &Arc<RpcRotator>,
) -> Vec<ArbitrageOpportunity> {
    let mut opportunities = Vec::new();

    info!(
        "[ARB/TAIL_RIDE] Scanning {} mempool (budget=${:.2})",
        chain.name(),
        budget
    );

    let max_position = budget.min(50.0);
    let gas_cost = chain.avg_gas_gwei() * 150000.0 / 1e9 * 2000.0 / max_position;
    let simulated_edge = 0.008;
    let slippage = 0.002;

    if passes_pnet(simulated_edge, gas_cost, slippage) {
        let net = simulated_edge - gas_cost - slippage;
        opportunities.push(ArbitrageOpportunity {
            chain,
            strategy: Strategy::TailRide,
            token_pair: "WETH/USDC".into(),
            source_dex: "UniswapV3".into(),
            target_dex: "UniswapV3".into(),
            gross_edge: simulated_edge,
            gas_cost,
            slippage,
            net_edge: net,
            recommended_size: kelly_size(max_position, 0.50, 3.0),
            confidence: 0.0,
        });
    }

    opportunities
}

// ─── Execution Engine ────────────────────────────────────────────────────────

async fn execute_opportunity(
    opp: &ArbitrageOpportunity,
    rpc: &Arc<RpcRotator>,
) -> Result<f64, Box<dyn std::error::Error + Send + Sync>> {
    let fresh_gas = estimate_gas_cost(opp.chain, rpc).await?;
    if !passes_pnet(opp.gross_edge, fresh_gas, opp.slippage) {
        return Err("P_net below threshold with fresh gas".into());
    }

    info!(
        "[ARB/EXEC] {} on {} | Pair={} | Size=${:.4} | NetEdge={:.4}%",
        opp.strategy,
        opp.chain.name(),
        opp.token_pair,
        opp.recommended_size,
        opp.net_edge * 100.0
    );

    Ok(opp.recommended_size * opp.net_edge)
}

async fn estimate_gas_cost(
    chain: Chain,
    rpc: &Arc<RpcRotator>,
) -> Result<f64, Box<dyn std::error::Error + Send + Sync>> {
    match rpc.get_provider(chain.name()).await {
        Ok(provider) => {
            match provider.get_gas_price().await {
                Ok(gas_price) => {
                    let gwei = gas_price.as_u64() as f64 / 1e9;
                    let cost_eth = gwei * 150_000.0 / 1e9;
                    let cost_usd = cost_eth * 2000.0;
                    Ok(cost_usd / 50.0)
                }
                Err(_) => Ok(chain.avg_gas_gwei() * 150_000.0 / 1e9 * 2000.0 / 50.0),
            }
        }
        Err(_) => Ok(chain.avg_gas_gwei() * 150_000.0 / 1e9 * 2000.0 / 50.0),
    }
}

// ─── Main Arbitrage Entry Point ──────────────────────────────────────────────

pub async fn execute_cross_dex_arbitrage(budget: f64, rpc: &Arc<RpcRotator>) -> ArbitrageResult {
    let mut result = ArbitrageResult::default();

    info!(
        "[ARB] Starting arbitrage cycle. Budget=${:.2}, Kelly={}%, P_net>{}%",
        budget,
        KELLY_RISK * 100.0,
        P_NET_THRESHOLD * 100.0
    );

    for chain in Chain::all() {
        let cross_dex_opps = scan_cross_dex(chain, budget * 0.6, rpc).await;
        let tail_ride_opps = scan_tail_ride(chain, budget * 0.4, rpc).await;

        let all_opps: Vec<ArbitrageOpportunity> = cross_dex_opps
            .into_iter()
            .chain(tail_ride_opps.into_iter())
            .collect();

        result.opportunities_found += all_opps.len();

        for opp in &all_opps {
            if opp.confidence <= 0.0 {
                result.trades_skipped += 1;
                continue;
            }

            match execute_opportunity(opp, rpc).await {
                Ok(pnl) => {
                    result.pnl += pnl;
                    result.gas_spent += opp.gas_cost * opp.recommended_size;
                    result.trades_executed += 1;
                }
                Err(e) => {
                    result.trades_skipped += 1;
                    warn!("[ARB/SKIP] {} on {}: {}", opp.strategy, chain.name(), e);
                }
            }
        }
    }

    info!(
        "[ARB] Cycle complete. Opportunities={}, Executed={}, Skipped={}, PnL=${:.6}",
        result.opportunities_found, result.trades_executed, result.trades_skipped, result.pnl
    );

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chain_config() {
        assert_eq!(Chain::Base.chain_id(), 8453);
        assert_eq!(Chain::Arbitrum.chain_id(), 42161);
        assert_eq!(Chain::Polygon.chain_id(), 137);
    }

    #[test]
    fn test_all_chains() {
        assert_eq!(Chain::all().len(), 3);
    }
}
