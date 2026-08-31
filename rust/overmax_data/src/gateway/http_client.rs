//! Unified Outbound HTTP Transport Layer.
//!
//! Provides an explicit, connection-pooled HTTP client struct with standardized
//! headers, User-Agent, and timeout profiles.

use crate::gateway::error::GatewayResult;
use reqwest::blocking::{Client, RequestBuilder};
use reqwest::header::{CACHE_CONTROL, PRAGMA, USER_AGENT};
use std::time::Duration;

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
pub const FAST_TIMEOUT: Duration = Duration::from_secs(5);
pub const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(30);

const APP_USER_AGENT: &str = concat!("Overmax/", env!("CARGO_PKG_VERSION"));

/// Unified Outbound HTTP Client.
///
/// Wraps `reqwest::blocking::Client` (which is internally an `Arc` pointer,
/// making cloning cheap) and injects standard headers and User-Agent.
#[derive(Clone, Debug)]
pub struct GatewayHttpClient {
    client: Client,
    user_agent: &'static str,
}

impl GatewayHttpClient {
    /// Creates a new HTTP client with default settings.
    pub fn new() -> Self {
        Self::with_user_agent(APP_USER_AGENT)
    }

    /// Creates a new HTTP client with a custom User-Agent.
    pub fn with_user_agent(user_agent: &'static str) -> Self {
        let client = Client::builder()
            .timeout(DEFAULT_TIMEOUT)
            .user_agent(user_agent)
            .build()
            .unwrap_or_else(|_| Client::new());

        Self { client, user_agent }
    }

    pub fn inner(&self) -> &Client {
        &self.client
    }

    pub fn user_agent(&self) -> &'static str {
        self.user_agent
    }

    /// Prepares a GET request with standard headers and optional timeout override.
    pub fn get(&self, url: &str, timeout: Option<Duration>) -> RequestBuilder {
        let mut req = self
            .client
            .get(url)
            .header(USER_AGENT, self.user_agent)
            .header(CACHE_CONTROL, "no-cache")
            .header(PRAGMA, "no-cache");

        if let Some(t) = timeout {
            req = req.timeout(t);
        }
        req
    }

    /// Prepares a POST request with standard headers and optional timeout override.
    pub fn post(&self, url: &str, timeout: Option<Duration>) -> RequestBuilder {
        let mut req = self
            .client
            .post(url)
            .header(USER_AGENT, self.user_agent);

        if let Some(t) = timeout {
            req = req.timeout(t);
        }
        req
    }

    /// Downloads raw bytes with specified timeout.
    pub fn download_bytes(&self, url: &str, timeout: Duration) -> GatewayResult<Vec<u8>> {
        let resp = self.get(url, Some(timeout)).send()?.error_for_status()?;
        let bytes = resp.bytes()?;
        Ok(bytes.to_vec())
    }

    /// Downloads text content with specified timeout and UTF-8 BOM stripping.
    pub fn download_text(&self, url: &str, timeout: Duration) -> GatewayResult<String> {
        let bytes = self.download_bytes(url, timeout)?;
        Ok(String::from_utf8_lossy(&bytes)
            .trim_start_matches('\u{feff}')
            .to_string())
    }
}

impl Default for GatewayHttpClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_client_initializes_with_correct_user_agent() {
        let client = GatewayHttpClient::new();
        assert!(client.user_agent().starts_with("Overmax/"));
    }
}
