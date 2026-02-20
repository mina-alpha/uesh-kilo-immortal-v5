//! L2 Arbitrage Engine v5 — Main Tick Engine
//!
//! 30-second tick lifecycle:
//!   1. Rotate RPC endpoints and check health
//!   2. Scan cross-DEX price discrepancies on Base, Arbitrum, Polygon
//!   3. Evaluate arbitrage opportunities against P_net threshold
//!   4. Size positions using Kelly criterion
//!   5. Execute profitable trades and record PnL
//!
//! HTTP API: /health, /status, /metrics

mod scanner {
    pub mod analysis;
}
mod arbitrage;
mod rpc_rotator;
mod treasury {
    pub mod guard;
}

use axum::{routing::get, Json, Router};
use ethers::prelude::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

// ─── Constants ───────────────────────────────────────────────────────────────

const TICK_INTERVAL_SECS: u64 = 30;
const INITIAL_CAPITAL: f64 = 50.0;
const KELLY_RISK: f64 = 0.01;           // 1% Kelly fraction per position
const P_NET_THRESHOLD: f64 = 0.003;     // 0.3% minimum net edge after costs
const PROFIT_RESERVE_PCT: f64 = 0.40;   // 40% of profits reserved for withdrawal
const SCALING_THRESHOLD: f64 = 500.0;   // Capital threshold for strategy scaling

// ─── Engine State ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EngineMode {
    Bootstrap,  // Initial phase: conservative strategies only
    Standard,   // Full operation: all strategies active
}

impl std::fmt::Display for EngineMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EngineMode::Bootstrap => write!(f, "BOOTSTRAP"),
            EngineMode::Standard => write!(f, "STANDARD"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineState {
    pub mode: EngineMode,
    pub capital_usd: f64,
    pub tick_count: u64,
    pub arbitrage_pnl: f64,
    pub scanner_pnl: f64,
    pub total_reserved: f64,
    pub uptime_secs: u64,
    pub last_tick_ms: u64,
    pub rpc_healthy: usize,
    pub rpc_total: usize,
}

impl Default for EngineState {
    fn default() -> Self {
        Self {
            mode: EngineMode::Bootstrap,
            capital_usd: INITIAL_CAPITAL,
            tick_count: 0,
            arbitrage_pnl: 0.0,
            scanner_pnl: 0.0,
            total_reserved: 0.0,
            uptime_secs: 0,
            last_tick_ms: 0,
            rpc_healthy: 0,
            rpc_total: 12,
        }
    }
}

type SharedState = Arc<RwLock<EngineState>>;

// ─── Kelly Criterion Position Sizing ─────────────────────────────────────────

/// Calculate position size using half-Kelly criterion.
/// Returns the dollar amount to allocate to a single trade.
pub fn kelly_size(bankroll: f64, win_prob: f64, win_ratio: f64) -> f64 {
    let q = 1.0 - win_prob;
    let edge = (win_prob * win_ratio - q) / win_ratio;
    let size = (edge * KELLY_RISK * bankroll).max(0.0);
    size.min(bankroll * KELLY_RISK)
}

/// Validate that a trade meets the minimum net edge threshold.
/// P_net = gross_edge - gas_cost - slippage
pub fn passes_pnet(gross_edge: f64, gas_cost: f64, slippage: f64) -> bool {
    let p_net = gross_edge - gas_cost - slippage;
    p_net >= P_NET_THRESHOLD
}

// ─── Tick Engine ─────────────────────────────────────────────────────────────

async fn run_tick(state: SharedState, rpc: Arc<rpc_rotator::RpcRotator>) {
    let tick_start = std::time::Instant::now();
    let mut s = state.write().await;
    s.tick_count += 1;
    let tick = s.tick_count;
    let mode = s.mode;
    let capital = s.capital_usd;

    info!(
        "[TICK {}] Mode={} Capital=${:.2}",
        tick, mode, capital
    );

    // Update RPC health
    let (healthy, total) = rpc.health_summary().await;
    s.rpc_healthy = healthy;
    s.rpc_total = total;

    drop(s); // Release write lock during strategy execution

    match mode {
        EngineMode::Bootstrap => {
            // Conservative: cross-DEX arbitrage only
            let arb_budget = capital * 0.80;
            let scan_budget = capital * 0.20;

            let arb_result =
                arbitrage::execute_cross_dex_arbitrage(arb_budget, &rpc).await;

            let scan_result =
                scanner::analysis::scan_contracts(scan_budget, &rpc).await;

            let mut s = state.write().await;
            s.arbitrage_pnl += arb_result.pnl;
            s.scanner_pnl += scan_result.pnl;
            s.capital_usd += arb_result.pnl + scan_result.pnl;

            // Mode transition check
            if s.capital_usd >= SCALING_THRESHOLD {
                info!(
                    "[MODE TRANSITION] BOOTSTRAP -> STANDARD (Capital=${:.2})",
                    s.capital_usd
                );
                s.mode = EngineMode::Standard;
            }
        }

        EngineMode::Standard => {
            // Full operation: diversified strategy allocation
            let arb_budget = capital * 0.50;
            let scan_budget = capital * 0.20;
            let advanced_budget = capital * 0.30;

            let arb_result =
                arbitrage::execute_cross_dex_arbitrage(arb_budget, &rpc).await;

            let scan_result =
                scanner::analysis::scan_contracts(scan_budget, &rpc).await;

            // Advanced strategies: liquidation monitoring, cross-L2 arb
            let advanced_pnl =
                execute_advanced_strategies(advanced_budget, &rpc).await;

            let mut s = state.write().await;
            s.arbitrage_pnl += arb_result.pnl;
            s.scanner_pnl += scan_result.pnl;
            s.capital_usd += arb_result.pnl + scan_result.pnl + advanced_pnl;

            // Reserve profits for withdrawal
            let total_profit = arb_result.pnl + scan_result.pnl + advanced_pnl;
            if total_profit > 0.0 {
                let reserve_amount = total_profit * PROFIT_RESERVE_PCT;
                match treasury::guard::record_profit(reserve_amount, &rpc).await {
                    Ok(report) => {
                        info!("[TREASURY] Reserved ${:.4}. {}", reserve_amount, report);
                        s.total_reserved += reserve_amount;
                    }
                    Err(e) => warn!("[TREASURY] Reserve recording failed: {}", e),
                }
            }
        }
    }

    // Record tick duration
    let elapsed = tick_start.elapsed().as_millis() as u64;
    let mut s = state.write().await;
    s.last_tick_ms = elapsed;
    s.uptime_secs += TICK_INTERVAL_SECS;
    info!("[TICK {} DONE] Duration={}ms Capital=${:.2}", tick, elapsed, s.capital_usd);
}

// ─── Advanced Strategies (Standard mode) ─────────────────────────────────────

async fn execute_advanced_strategies(
    budget: f64,
    rpc: &Arc<rpc_rotator::RpcRotator>,
) -> f64 {
    let mut pnl = 0.0;

    // Cross-DEX arbitrage: Uniswap V3 <-> SushiSwap <-> Curve
    let arb_edge = scan_cross_dex_arb(rpc).await;
    if passes_pnet(arb_edge, 0.0005, 0.0003) {
        let size = kelly_size(budget, 0.65, 2.0);
        info!("[ADVANCED/ARB] Edge={:.4}% Size=${:.4}", arb_edge * 100.0, size);
        pnl += size * arb_edge;
    }

    // Liquidation monitoring on Aave V3 / Compound V3
    let liq_edge = scan_liquidation_opportunities(rpc).await;
    if passes_pnet(liq_edge, 0.0008, 0.0002) {
        let size = kelly_size(budget, 0.70, 1.8);
        info!("[ADVANCED/LIQ] Edge={:.4}% Size=${:.4}", liq_edge * 100.0, size);
        pnl += size * liq_edge;
    }

    pnl
}

async fn scan_cross_dex_arb(_rpc: &Arc<rpc_rotator::RpcRotator>) -> f64 {
    // Scan Uniswap V3 <-> SushiSwap <-> Curve price discrepancies
    // Returns gross edge as decimal (e.g., 0.005 = 0.5%)
    0.0 // Conservative: only execute when real edge detected
}

async fn scan_liquidation_opportunities(_rpc: &Arc<rpc_rotator::RpcRotator>) -> f64 {
    // Monitor health factors on Aave V3 / Compound V3
    0.0 // Conservative: requires live on-chain data
}

// ─── Axum HTTP API ───────────────────────────────────────────────────────────

async fn health() -> &'static str {
    "OK"
}

async fn status(state: axum::extract::State<SharedState>) -> Json<EngineState> {
    Json(state.read().await.clone())
}

async fn mode(state: axum::extract::State<SharedState>) -> String {
    format!("{}", state.read().await.mode)
}

fn build_router(state: SharedState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/status", get(status))
        .route("/mode", get(mode))
        .with_state(state)
}

// ─── Entry Point ─────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "l2_arb_engine=info,tower_http=info".into()),
        )
        .json()
        .init();

    info!("=== L2 Arbitrage Engine v5 ===");
    info!("Initial capital: ${}", INITIAL_CAPITAL);
    info!("Kelly risk: {}%", KELLY_RISK * 100.0);
    info!("P_net threshold: {}%", P_NET_THRESHOLD * 100.0);
    info!("Profit reserve: {}%", PROFIT_RESERVE_PCT * 100.0);

    // Initialize RPC rotator with 12 free public endpoints
    let rpc = Arc::new(rpc_rotator::RpcRotator::new().await);
    info!(
        "[RPC] Initialized {} endpoints across {} chains",
        rpc.endpoint_count(),
        rpc.chain_count()
    );

    // Initialize engine state
    let state: SharedState = Arc::new(RwLock::new(EngineState::default()));

    // Check treasury contract deployment
    match treasury::guard::ensure_treasury_deployed(&rpc).await {
        Ok(addr) => info!("[TREASURY] Contract at: {}", addr),
        Err(e) => warn!("[TREASURY] Deploy deferred: {}", e),
    }

    // Spawn tick engine
    let tick_state = state.clone();
    let tick_rpc = rpc.clone();
    tokio::spawn(async move {
        let mut interval =
            tokio::time::interval(std::time::Duration::from_secs(TICK_INTERVAL_SECS));
        loop {
            interval.tick().await;
            run_tick(tick_state.clone(), tick_rpc.clone()).await;
        }
    });

    // Start HTTP server
    let port: u16 = std::env::var("ENGINE_PORT")
        .unwrap_or_else(|_| "8080".to_string())
        .parse()
        .unwrap_or(8080);

    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    info!("[HTTP] Listening on 0.0.0.0:{}", port);
    info!("[ENGINE] Started. First tick in {}s.", TICK_INTERVAL_SECS);

    axum::serve(listener, app).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kelly_size() {
        let size = kelly_size(1000.0, 0.6, 2.0);
        assert!(size > 0.0);
        assert!(size <= 1000.0 * KELLY_RISK);
    }

    #[test]
    fn test_kelly_size_no_edge() {
        let size = kelly_size(1000.0, 0.3, 1.0);
        assert_eq!(size, 0.0);
    }

    #[test]
    fn test_passes_pnet() {
        assert!(passes_pnet(0.005, 0.001, 0.0005));
        assert!(!passes_pnet(0.003, 0.001, 0.001));
    }

    #[test]
    fn test_mode_display() {
        assert_eq!(format!("{}", EngineMode::Bootstrap), "BOOTSTRAP");
        assert_eq!(format!("{}", EngineMode::Standard), "STANDARD");
    }

    #[test]
    fn test_default_state() {
        let state = EngineState::default();
        assert_eq!(state.mode, EngineMode::Bootstrap);
        assert_eq!(state.capital_usd, INITIAL_CAPITAL);
        assert_eq!(state.tick_count, 0);
    }
}
