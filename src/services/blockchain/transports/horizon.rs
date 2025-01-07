use crate::models::Network;

use super::super::BlockChainError;
use serde_json::{json, Value};
use stellar_horizon::{
    api::root,
    client::{HorizonClient as HorizonClientTrait, HorizonHttpClient},
};

pub struct HorizonTransportClient {
    pub client: HorizonHttpClient,
    pub url: String,
}

impl HorizonTransportClient {
    pub async fn new(network: &Network) -> Result<Self, BlockChainError> {
        // Filter horizon URLs with weight > 0 and sort by weight descending
        let mut horizon_urls: Vec<_> = network
            .rpc_urls
            .iter()
            .filter(|rpc_url| rpc_url.type_ == "horizon" && rpc_url.weight > 0)
            .collect();

        horizon_urls.sort_by(|a, b| b.weight.cmp(&a.weight));

        for rpc_url in horizon_urls {
            match HorizonHttpClient::new_from_str(&rpc_url.url) {
                Ok(client) => {
                    let request = root::root();
                    // Test connection by fetching root info
                    match client.request(request).await {
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
            "All Horizon RPC URLs failed to connect".to_string(),
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
