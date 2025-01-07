use super::super::BlockChainError;
use crate::models::Network;

use serde_json::{json, Value};
use stellar_rpc_client::Client as StellarHttpClient;

pub struct StellarTransportClient {
    pub client: StellarHttpClient,
    pub url: String,
}

impl StellarTransportClient {
    pub async fn new(network: &Network) -> Result<Self, BlockChainError> {
        // Filter stellar URLs with weight > 0 and sort by weight descending
        let mut stellar_urls: Vec<_> = network
            .rpc_urls
            .iter()
            .filter(|rpc_url| rpc_url.type_ == "rpc" && rpc_url.weight > 0)
            .collect();

        stellar_urls.sort_by(|a, b| b.weight.cmp(&a.weight));

        for rpc_url in stellar_urls {
            match StellarHttpClient::new(rpc_url.url.as_str()) {
                Ok(client) => {
                    // Test connection by fetching network info
                    match client.get_network().await {
                        Ok(_) => {
                            return Ok(Self {
                                client,
                                url: rpc_url.url.clone(),
                            })
                        }
                        Err(_) => continue,
                    }
                }
                Err(_) => continue,
            }
        }

        Err(BlockChainError::connection_error(
            "All Stellar RPC URLs failed to connect".to_string(),
        ))
    }

    pub async fn send_raw_request(
        &self,
        method: &str,
        params: Value,
    ) -> Result<Value, BlockChainError> {
        let client = reqwest::Client::new();
        let url = self.url.clone();

        // Construct the JSON-RPC request
        let request_body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params
        });

        let response = client
            .post(url)
            .header("Content-Type", "application/json")
            .json(&request_body) // Use .json() instead of .body() for proper serialization
            .send()
            .await
            .map_err(|e| BlockChainError::connection_error(e.to_string()))?;

        let json: Value = response
            .json()
            .await
            .map_err(|e| BlockChainError::connection_error(e.to_string()))?;

        Ok(json)
    }
}
