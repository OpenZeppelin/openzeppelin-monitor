use serde_json::{json, Value};
use web3::{transports::Http, Web3};

use super::super::BlockChainError;
use crate::models::Network;

pub struct Web3TransportClient {
    pub client: Web3<Http>,
    pub url: String,
}

impl Web3TransportClient {
    pub async fn new(network: &Network) -> Result<Self, BlockChainError> {
        // Filter web3 URLs with weight > 0 and sort by weight descending
        let mut rpc_urls: Vec<_> = network
            .rpc_urls
            .iter()
            .filter(|rpc_url| rpc_url.type_ == "rpc" && rpc_url.weight > 0)
            .collect();

        rpc_urls.sort_by(|a, b| b.weight.cmp(&a.weight));

        for rpc_url in rpc_urls {
            match Http::new(rpc_url.url.as_str()) {
                Ok(transport) => {
                    let client = Web3::new(transport);
                    match client.net().version().await {
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
            "All RPC URLs failed to connect".to_string(),
        ))
    }

    pub async fn send_raw_request(
        &self,
        method: &str,
        params: Vec<Value>,
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
