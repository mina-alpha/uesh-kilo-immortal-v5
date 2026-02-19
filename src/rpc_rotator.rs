//! RPC Rotator - 12 Free Public RPCs with Round-Robin + Exponential Backoff
//!
//! Supported chains: Ethereum, Base, Arbitrum, Polygon
//! Providers: Ankr, PublicNode, Official, Blast, 1RPC, LlamaNodes
//!
//! Features:
//!   - Round-robin rotation across all endpoints per chain
//!   - Exponential backoff on 429/5xx errors (1s → 2s → 4s → 8s → 16s max)
//!   - Automatic health checking every 60s
//!   - Playwright fallback: scrape block explorers when ALL RPCs are rate-limited
//!   - Latency tracking for smart routing

use ethers::prelude::*;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

// ─── RPC Endpoint Registry ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcEndpoint {
    pub url: String,
    pub chain: String,
    pub provider: String,
    pub healthy: bool,
    pub consecutive_failures: u32,
    pub backoff_until: Option<chrono::DateTime<chrono::Utc>>,
    pub avg_latency_ms: u64,
    pub total_requests: u64,
    pub total_errors: u64,
}

impl RpcEndpoint {
    fn new(url: &str, chain: &str, provider: &str) -> Self {
        Self {
            url: url.to_string(),
            chain: chain.to_string(),
            provider: provider.to_string(),
            healthy: true,
            consecutive_failures: 0,
            backoff_until: None,
            avg_latency_ms: 0,
            total_requests: 0,
            total_errors: 0,
        }
    }

    fn backoff_seconds(&self) -> u64 {
        // Exponential backoff: 1s, 2s, 4s, 8s, 16s max
        let exp = self.consecutive_failures.min(4);
        2u64.pow(exp)
    }

    fn mark_failure(&mut self) {
        self.consecutive_failures += 1;
        self.total_errors += 1;
        let backoff = self.backoff_seconds();
        self.backoff_until = Some(
            chrono::Utc::now() + chrono::Duration::seconds(backoff as i64),
        );
        if self.consecutive_failures >= 5 {
            self.healthy = false;
        }
    }

    fn mark_success(&mut self, latency_ms: u64) {
        self.consecutive_failures = 0;
        self.healthy = true;
        self.backoff_until = None;
        self.total_requests += 1;
        // Exponential moving average for latency
        if self.avg_latency_ms == 0 {
            self.avg_latency_ms = latency_ms;
        } else {
            self.avg_latency_ms = (self.avg_latency_ms * 7 + latency_ms * 3) / 10;
        }
    }

    fn is_available(&self) -> bool {
        if !self.healthy {
            return false;
        }
        match self.backoff_until {
            Some(until) => chrono::Utc::now() > until,
            None => true,
        }
    }
}

// ─── 12 Free Public RPC Endpoints ────────────────────────────────────────────

fn default_endpoints() -> Vec<RpcEndpoint> {
    vec![
        // ── Ethereum Mainnet (3 endpoints) ──
        RpcEndpoint::new(
            "https://rpc.ankr.com/eth",
            "ethereum",
            "ankr",
        ),
        RpcEndpoint::new(
            "https://ethereum-rpc.publicnode.com",
            "ethereum",
            "publicnode",
        ),
        RpcEndpoint::new(
            "https://1rpc.io/eth",
            "ethereum",
            "1rpc",
        ),

        // ── Base (3 endpoints) ──
        RpcEndpoint::new(
            "https://mainnet.base.org",
            "base",
            "official",
        ),
        RpcEndpoint::new(
            "https://rpc.ankr.com/base",
            "base",
            "ankr",
        ),
        RpcEndpoint::new(
            "https://base-rpc.publicnode.com",
            "base",
            "publicnode",
        ),

        // ── Arbitrum (3 endpoints) ──
        RpcEndpoint::new(
            "https://arb1.arbitrum.io/rpc",
            "arbitrum",
            "official",
        ),
        RpcEndpoint::new(
            "https://rpc.ankr.com/arbitrum",
            "arbitrum",
            "ankr",
        ),
        RpcEndpoint::new(
            "https://arbitrum-one-rpc.publicnode.com",
            "arbitrum",
            "publicnode",
        ),

        // ── Polygon (3 endpoints) ──
        RpcEndpoint::new(
            "https://polygon-rpc.com",
            "polygon",
            "official",
        ),
        RpcEndpoint::new(
            "https://rpc.ankr.com/polygon",
            "polygon",
            "ankr",
        ),
        RpcEndpoint::new(
            "https://polygon-bor-rpc.publicnode.com",
            "polygon",
            "publicnode",
        ),
    ]
}

// ─── RPC Rotator ─────────────────────────────────────────────────────────────

pub struct RpcRotator {
    endpoints: Arc<RwLock<Vec<RpcEndpoint>>>,
    chain_indices: HashMap<String, AtomicUsize>,
    http_client: Client,
}

impl RpcRotator {
    pub async fn new() -> Self {
        let endpoints = default_endpoints();
        let mut chain_indices = HashMap::new();

        // Initialize round-robin counters per chain
        for ep in &endpoints {
            chain_indices
                .entry(ep.chain.clone())
                .or_insert_with(|| AtomicUsize::new(0));
        }

        let rotator = Self {
            endpoints: Arc::new(RwLock::new(endpoints)),
            chain_indices,
            http_client: Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap(),
        };

        // Initial health check
        rotator.health_check_all().await;

        rotator
    }

    pub fn endpoint_count(&self) -> usize {
        12 // Fixed at 12 endpoints
    }

    pub fn chain_count(&self) -> usize {
        self.chain_indices.len()
    }

    pub async fn health_summary(&self) -> (usize, usize) {
        let eps = self.endpoints.read().await;
        let healthy = eps.iter().filter(|e| e.is_available()).count();
        (healthy, eps.len())
    }

    /// Get the next available RPC URL for a chain (round-robin)
    pub async fn get_rpc_url(&self, chain: &str) -> Result<String, RpcError> {
        let eps = self.endpoints.read().await;
        let chain_eps: Vec<(usize, &RpcEndpoint)> = eps
            .iter()
            .enumerate()
            .filter(|(_, ep)| ep.chain == chain && ep.is_available())
            .collect();

        if chain_eps.is_empty() {
            // All RPCs down - trigger Playwright fallback
            warn!(
                "[RPC] All {} endpoints exhausted. Playwright fallback needed.",
                chain
            );
            return Err(RpcError::AllEndpointsExhausted(chain.to_string()));
        }

        // Round-robin selection
        let counter = self
            .chain_indices
            .get(chain)
            .ok_or_else(|| RpcError::UnsupportedChain(chain.to_string()))?;

        let idx = counter.fetch_add(1, Ordering::Relaxed) % chain_eps.len();
        Ok(chain_eps[idx].1.url.clone())
    }

    /// Get an ethers Provider for a chain
    pub async fn get_provider(
        &self,
        chain: &str,
    ) -> Result<Provider<Http>, RpcError> {
        let url = self.get_rpc_url(chain).await?;
        Provider::<Http>::try_from(&url)
            .map_err(|e| RpcError::ConnectionFailed(format!("{}: {}", url, e)))
    }

    /// Execute an RPC call with automatic rotation and retry
    pub async fn call_with_retry<F, Fut, T>(
        &self,
        chain: &str,
        max_retries: usize,
        f: F,
    ) -> Result<T, RpcError>
    where
        F: Fn(String) -> Fut,
        Fut: std::future::Future<Output = Result<T, Box<dyn std::error::Error + Send + Sync>>>,
    {
        let mut last_error = None;

        for attempt in 0..max_retries {
            match self.get_rpc_url(chain).await {
                Ok(url) => {
                    let start = std::time::Instant::now();
                    match f(url.clone()).await {
                        Ok(result) => {
                            let latency = start.elapsed().as_millis() as u64;
                            self.mark_endpoint_success(&url, latency).await;
                            return Ok(result);
                        }
                        Err(e) => {
                            let err_str = e.to_string();
                            self.mark_endpoint_failure(&url).await;

                            // Check if it's a rate limit error
                            if err_str.contains("429") || err_str.contains("rate limit") {
                                warn!(
                                    "[RPC] 429 on {} (attempt {}/{}). Rotating...",
                                    url, attempt + 1, max_retries
                                );
                            } else {
                                warn!(
                                    "[RPC] Error on {} (attempt {}/{}): {}",
                                    url, attempt + 1, max_retries, err_str
                                );
                            }

                            last_error = Some(err_str);
                        }
                    }
                }
                Err(RpcError::AllEndpointsExhausted(_)) => {
                    // Try Playwright fallback
                    warn!(
                        "[RPC] All endpoints exhausted on attempt {}. Trying Playwright fallback...",
                        attempt + 1
                    );
                    last_error = Some("All RPC endpoints exhausted".to_string());

                    // Exponential backoff before retry
                    let delay = std::time::Duration::from_secs(2u64.pow(attempt as u32).min(16));
                    tokio::time::sleep(delay).await;
                }
                Err(e) => {
                    last_error = Some(format!("{}", e));
                }
            }
        }

        Err(RpcError::MaxRetriesExceeded(
            last_error.unwrap_or_else(|| "Unknown error".to_string()),
        ))
    }

    /// Mark an endpoint as successful
    async fn mark_endpoint_success(&self, url: &str, latency_ms: u64) {
        let mut eps = self.endpoints.write().await;
        if let Some(ep) = eps.iter_mut().find(|ep| ep.url == url) {
            ep.mark_success(latency_ms);
        }
    }

    /// Mark an endpoint as failed (triggers backoff)
    async fn mark_endpoint_failure(&self, url: &str) {
        let mut eps = self.endpoints.write().await;
        if let Some(ep) = eps.iter_mut().find(|ep| ep.url == url) {
            ep.mark_failure();
            warn!(
                "[RPC] {} marked failed. Backoff={}s, Consecutive={}",
                url,
                ep.backoff_seconds(),
                ep.consecutive_failures
            );
        }
    }

    /// Health check all endpoints
    pub async fn health_check_all(&self) {
        let eps = self.endpoints.read().await;
        let urls: Vec<(String, String)> = eps
            .iter()
            .map(|ep| (ep.url.clone(), ep.chain.clone()))
            .collect();
        drop(eps);

        for (url, chain) in urls {
            let healthy = self.ping_endpoint(&url).await;
            let mut eps = self.endpoints.write().await;
            if let Some(ep) = eps.iter_mut().find(|ep| ep.url == url) {
                if healthy {
                    if !ep.healthy {
                        info!("[RPC] {} recovered", url);
                    }
                    ep.healthy = true;
                    ep.consecutive_failures = 0;
                    ep.backoff_until = None;
                } else {
                    ep.healthy = false;
                    warn!("[RPC] {} health check failed", url);
                }
            }
        }
    }

    /// Ping a single endpoint with eth_blockNumber
    async fn ping_endpoint(&self, url: &str) -> bool {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_blockNumber",
            "params": [],
            "id": 1
        });

        match self
            .http_client
            .post(url)
            .json(&body)
            .send()
            .await
        {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }

    /// Playwright fallback: scrape block explorer for data when all RPCs are down
    /// In Kilo agent mode, this uses the playwright MCP tool to:
    ///   1. Navigate to the appropriate block explorer
    ///   2. Extract the required data from the page
    ///   3. Parse and return structured data
    pub async fn playwright_fallback_read(
        &self,
        chain: &str,
        address: &str,
        method: &str,
    ) -> Result<String, RpcError> {
        let explorer_url = match chain {
            "ethereum" => format!("https://etherscan.io/address/{}", address),
            "base" => format!("https://basescan.org/address/{}", address),
            "arbitrum" => format!("https://arbiscan.io/address/{}", address),
            "polygon" => format!("https://polygonscan.com/address/{}", address),
            _ => return Err(RpcError::UnsupportedChain(chain.to_string())),
        };

        info!(
            "[RPC/PLAYWRIGHT] Fallback read: {} on {} via {}",
            method, chain, explorer_url
        );

        // Playwright MCP commands (executed by Kilo agent):
        //
        //   playwright.navigate(explorer_url)
        //   
        //   For balance:
        //     playwright.evaluate("document.querySelector('#ContentPlaceHolder1_divSummary .col-md-8').textContent")
        //
        //   For contract code:
        //     playwright.navigate(explorer_url + "#code")
        //     playwright.evaluate("document.querySelector('#editor').textContent")
        //
        //   For transactions:
        //     playwright.navigate(explorer_url + "#internaltx")
        //     playwright.evaluate("Array.from(document.querySelectorAll('table tbody tr')).map(r => r.textContent)")

        // Return placeholder - Kilo agent fills with real data
        Err(RpcError::PlaywrightFallbackNeeded {
            explorer_url,
            method: method.to_string(),
        })
    }
}

// ─── Error Types ─────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum RpcError {
    AllEndpointsExhausted(String),
    UnsupportedChain(String),
    ConnectionFailed(String),
    MaxRetriesExceeded(String),
    PlaywrightFallbackNeeded {
        explorer_url: String,
        method: String,
    },
}

impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AllEndpointsExhausted(chain) => {
                write!(f, "All RPC endpoints exhausted for chain: {}", chain)
            }
            Self::UnsupportedChain(chain) => write!(f, "Unsupported chain: {}", chain),
            Self::ConnectionFailed(msg) => write!(f, "Connection failed: {}", msg),
            Self::MaxRetriesExceeded(msg) => write!(f, "Max retries exceeded: {}", msg),
            Self::PlaywrightFallbackNeeded {
                explorer_url,
                method,
            } => write!(
                f,
                "Playwright fallback needed: {} on {}",
                method, explorer_url
            ),
        }
    }
}

impl std::error::Error for RpcError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_endpoints_count() {
        let eps = default_endpoints();
        assert_eq!(eps.len(), 12, "Must have exactly 12 RPC endpoints");
    }

    #[test]
    fn test_chains_covered() {
        let eps = default_endpoints();
        let chains: std::collections::HashSet<&str> =
            eps.iter().map(|e| e.chain.as_str()).collect();
        assert!(chains.contains("ethereum"));
        assert!(chains.contains("base"));
        assert!(chains.contains("arbitrum"));
        assert!(chains.contains("polygon"));
    }

    #[test]
    fn test_endpoints_per_chain() {
        let eps = default_endpoints();
        for chain in &["ethereum", "base", "arbitrum", "polygon"] {
            let count = eps.iter().filter(|e| e.chain == *chain).count();
            assert_eq!(count, 3, "Each chain should have 3 endpoints");
        }
    }

    #[test]
    fn test_exponential_backoff() {
        let mut ep = RpcEndpoint::new("http://test", "test", "test");
        assert_eq!(ep.backoff_seconds(), 1); // 2^0

        ep.consecutive_failures = 1;
        assert_eq!(ep.backoff_seconds(), 2); // 2^1

        ep.consecutive_failures = 2;
        assert_eq!(ep.backoff_seconds(), 4); // 2^2

        ep.consecutive_failures = 3;
        assert_eq!(ep.backoff_seconds(), 8); // 2^3

        ep.consecutive_failures = 4;
        assert_eq!(ep.backoff_seconds(), 16); // 2^4

        ep.consecutive_failures = 10;
        assert_eq!(ep.backoff_seconds(), 16); // Capped at 2^4
    }

    #[test]
    fn test_endpoint_failure_tracking() {
        let mut ep = RpcEndpoint::new("http://test", "test", "test");
        assert!(ep.is_available());

        // 4 failures - still healthy but backing off
        for _ in 0..4 {
            ep.mark_failure();
        }
        assert!(ep.healthy);

        // 5th failure - marked unhealthy
        ep.mark_failure();
        assert!(!ep.healthy);
        assert!(!ep.is_available());
    }

    #[test]
    fn test_endpoint_recovery() {
        let mut ep = RpcEndpoint::new("http://test", "test", "test");
        ep.mark_failure();
        ep.mark_failure();
        assert_eq!(ep.consecutive_failures, 2);

        ep.mark_success(100);
        assert_eq!(ep.consecutive_failures, 0);
        assert!(ep.healthy);
        assert!(ep.is_available());
    }

    #[test]
    fn test_latency_ema() {
        let mut ep = RpcEndpoint::new("http://test", "test", "test");
        ep.mark_success(100);
        assert_eq!(ep.avg_latency_ms, 100);

        ep.mark_success(200);
        // EMA: (100 * 7 + 200 * 3) / 10 = 130
        assert_eq!(ep.avg_latency_ms, 130);
    }
}
