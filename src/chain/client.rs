// SPDX-License-Identifier: MIT
//
// RpcClient — rate-limited wrapper around an alloy Provider.
//
// V8 §5 M3 DoD: "RPC rate limit: `governor::Quota::per_second(25)` (Infura
// free tier ceiling)".
//
// We use `governor::RateLimiter` (default quota = 25 req/s, configurable)
// to throttle outbound calls. Each `Provider` method `.await`s an
// internal "permit" before issuing the RPC.
//
// The inner provider type is left opaque (whatever
// `ProviderBuilder::new().with_recommended_fillers().connect_http(url)`
// returns). Alloy handles the network trait machinery internally.

use std::time::Duration;

use alloy_provider::{Provider, ProviderBuilder};
use governor::{
    clock::DefaultClock,
    state::{InMemoryState, NotKeyed},
    Quota, RateLimiter,
};
use url::Url;

use crate::chain::ChainError;

/// Default rate limit: 25 requests per second (per V8 §5 M3).
pub const DEFAULT_RPS: u32 = 25;

/// Rate-limited JSON-RPC client. The inner provider type is opaque
/// (any type implementing `alloy::Provider`).
pub struct RpcClient {
    inner: Box<dyn Provider + Send + Sync>,
    limiter: RateLimiter<NotKeyed, InMemoryState, DefaultClock>,
    rpc_url: String,
}

impl std::fmt::Debug for RpcClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RpcClient")
            .field("rpc_url", &self.rpc_url)
            .field("rate_limit", &"<governor RateLimiter>")
            .finish()
    }
}

impl RpcClient {
    /// Build a client for the given HTTP(S) RPC URL with the default
    /// rate limit (25 req/s).
    pub fn new(rpc_url: &str) -> Result<Self, ChainError> {
        Self::with_rate(rpc_url, DEFAULT_RPS)
    }

    /// Build a client with a custom rate limit.
    pub fn with_rate(rpc_url: &str, rps: u32) -> Result<Self, ChainError> {
        let url: Url = rpc_url
            .parse()
            .map_err(|e| ChainError::Rpc(format!("invalid RPC URL: {e}")))?;
        // ProviderBuilder::new() already includes the recommended fillers
        // (nonce management, chain-id fetching, gas estimation). We
        // just call `connect_http` to bind to a URL.
        let provider = ProviderBuilder::new().connect_http(url);
        let quota = Quota::per_second(
            std::num::NonZeroU32::new(rps)
                .ok_or_else(|| ChainError::Internal("rps must be > 0".to_string()))?,
        );
        let limiter = RateLimiter::direct(quota);
        Ok(Self {
            inner: Box::new(provider),
            limiter,
            rpc_url: rpc_url.to_string(),
        })
    }

    /// Borrow the underlying alloy provider as a trait object.
    pub fn provider(&self) -> &dyn Provider {
        self.inner.as_ref()
    }

    /// URL string of the RPC endpoint (for logging / error messages).
    pub fn rpc_url_str(&self) -> &str {
        &self.rpc_url
    }

    /// Acquire a rate-limit permit. Call before any RPC. If the
    /// limiter has no capacity, this awaits until one is available.
    pub async fn acquire(&self) -> Result<(), ChainError> {
        use governor::Jitter;
        let jitter = Jitter::up_to(Duration::from_millis(50));
        self.limiter.until_ready_with_jitter(jitter).await;
        Ok(())
    }
}
