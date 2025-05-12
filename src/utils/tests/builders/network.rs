//! Test helper utilities for Network configuration
//!
//! - `NetworkBuilder`: Builder for creating test Network instances

use crate::models::{BlockChainType, Network, RpcUrl, SecretString, SecretValue};

/// Builder for creating test Network instances
pub struct NetworkBuilder {
	name: String,
	slug: String,
	network_type: BlockChainType,
	chain_id: Option<u64>,
	network_passphrase: Option<String>,
	store_blocks: Option<bool>,
	rpc_urls: Vec<RpcUrl>,
	block_time_ms: u64,
	confirmation_blocks: u64,
	cron_schedule: String,
	max_past_blocks: Option<u64>,
}

impl Default for NetworkBuilder {
	fn default() -> Self {
		Self {
			name: "Test Network".to_string(),
			slug: "test_network".to_string(),
			network_type: BlockChainType::EVM,
			chain_id: Some(1),
			network_passphrase: None,
			store_blocks: Some(true),
			rpc_urls: vec![RpcUrl {
				type_: "rpc".to_string(),
				url: SecretValue::Plain(SecretString::new("https://test.network".to_string())),
				weight: 100,
			}],
			block_time_ms: 1000,
			confirmation_blocks: 1,
			cron_schedule: "0 */5 * * * *".to_string(),
			max_past_blocks: Some(10),
		}
	}
}

impl NetworkBuilder {
	pub fn new() -> Self {
		Self::default()
	}

	pub fn name(mut self, name: &str) -> Self {
		self.name = name.to_string();
		self
	}

	pub fn slug(mut self, slug: &str) -> Self {
		self.slug = slug.to_string();
		self
	}

	pub fn network_type(mut self, network_type: BlockChainType) -> Self {
		self.network_type = network_type;
		self
	}

	pub fn chain_id(mut self, chain_id: u64) -> Self {
		self.chain_id = Some(chain_id);
		self
	}

	pub fn network_passphrase(mut self, passphrase: &str) -> Self {
		self.network_passphrase = Some(passphrase.to_string());
		self
	}

	pub fn store_blocks(mut self, store: bool) -> Self {
		self.store_blocks = Some(store);
		self
	}

	pub fn rpc_url(mut self, url: &str) -> Self {
		self.rpc_urls = vec![RpcUrl {
			type_: "rpc".to_string(),
			url: SecretValue::Plain(SecretString::new(url.to_string())),
			weight: 100,
		}];
		self
	}

	pub fn rpc_urls(mut self, urls: Vec<&str>) -> Self {
		self.rpc_urls = urls
			.into_iter()
			.map(|url| RpcUrl {
				type_: "rpc".to_string(),
				url: SecretValue::Plain(SecretString::new(url.to_string())),
				weight: 100,
			})
			.collect();
		self
	}

	pub fn add_rpc_url(mut self, url: &str, type_: &str, weight: u32) -> Self {
		self.rpc_urls.push(RpcUrl {
			type_: type_.to_string(),
			url: SecretValue::Plain(SecretString::new(url.to_string())),
			weight,
		});
		self
	}

	pub fn add_secret_rpc_url(mut self, url: SecretValue, type_: &str, weight: u32) -> Self {
		self.rpc_urls.push(RpcUrl {
			type_: type_.to_string(),
			url,
			weight,
		});
		self
	}

	pub fn clear_rpc_urls(mut self) -> Self {
		self.rpc_urls.clear();
		self
	}

	pub fn block_time_ms(mut self, block_time: u64) -> Self {
		self.block_time_ms = block_time;
		self
	}

	pub fn confirmation_blocks(mut self, blocks: u64) -> Self {
		self.confirmation_blocks = blocks;
		self
	}

	pub fn cron_schedule(mut self, schedule: &str) -> Self {
		self.cron_schedule = schedule.to_string();
		self
	}

	pub fn max_past_blocks(mut self, blocks: u64) -> Self {
		self.max_past_blocks = Some(blocks);
		self
	}

	pub fn build(self) -> Network {
		Network {
			name: self.name,
			slug: self.slug,
			network_type: self.network_type,
			chain_id: self.chain_id,
			network_passphrase: self.network_passphrase,
			store_blocks: self.store_blocks,
			rpc_urls: self.rpc_urls,
			block_time_ms: self.block_time_ms,
			confirmation_blocks: self.confirmation_blocks,
			cron_schedule: self.cron_schedule,
			max_past_blocks: self.max_past_blocks,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_default_network() {
		let network = NetworkBuilder::new().build();

		assert_eq!(network.name, "Test Network");
		assert_eq!(network.slug, "test_network");
		assert_eq!(network.network_type, BlockChainType::EVM);
		assert_eq!(network.chain_id, Some(1));
		assert_eq!(network.network_passphrase, None);
		assert_eq!(network.store_blocks, Some(true));
		assert_eq!(network.block_time_ms, 1000);
		assert_eq!(network.confirmation_blocks, 1);
		assert_eq!(network.cron_schedule, "0 */5 * * * *");
		assert_eq!(network.max_past_blocks, Some(10));

		// Check default RPC URL
		assert_eq!(network.rpc_urls.len(), 1);
		assert_eq!(
			network.rpc_urls[0].url.as_ref().to_string(),
			"https://test.network".to_string()
		);
		assert_eq!(network.rpc_urls[0].type_, "rpc");
		assert_eq!(network.rpc_urls[0].weight, 100);
	}

	#[test]
	fn test_basic_builder_methods() {
		let network = NetworkBuilder::new()
			.name("Ethereum")
			.slug("eth")
			.network_type(BlockChainType::EVM)
			.chain_id(1)
			.store_blocks(true)
			.block_time_ms(15000)
			.confirmation_blocks(12)
			.build();

		assert_eq!(network.name, "Ethereum");
		assert_eq!(network.slug, "eth");
		assert_eq!(network.network_type, BlockChainType::EVM);
		assert_eq!(network.chain_id, Some(1));
		assert_eq!(network.store_blocks, Some(true));
		assert_eq!(network.block_time_ms, 15000);
		assert_eq!(network.confirmation_blocks, 12);
	}

	#[test]
	fn test_rpc_url_methods() {
		let network = NetworkBuilder::new()
			.clear_rpc_urls()
			.add_rpc_url("https://rpc1.example.com", "http", 50)
			.add_rpc_url("https://rpc2.example.com", "ws", 50)
			.build();

		assert_eq!(network.rpc_urls.len(), 2);
		assert_eq!(
			network.rpc_urls[0].url.as_ref().to_string(),
			"https://rpc1.example.com".to_string()
		);
		assert_eq!(network.rpc_urls[0].type_, "http");
		assert_eq!(network.rpc_urls[0].weight, 50);
		assert_eq!(
			network.rpc_urls[1].url.as_ref().to_string(),
			"https://rpc2.example.com".to_string()
		);
		assert_eq!(network.rpc_urls[1].type_, "ws");
		assert_eq!(network.rpc_urls[1].weight, 50);
	}

	#[test]
	fn test_secret_rpc_url() {
		let network = NetworkBuilder::new()
			.add_secret_rpc_url(
				SecretValue::Plain(SecretString::new("https://rpc1.example.com".to_string())),
				"rpc",
				50,
			)
			.build();

		assert_eq!(network.rpc_urls.len(), 2);
		assert_eq!(
			network.rpc_urls[1].url.as_ref().to_string(),
			"https://rpc1.example.com".to_string()
		);
		assert_eq!(network.rpc_urls[1].type_, "rpc");
	}

	#[test]
	fn test_rpc_urls_bulk_set() {
		let network = NetworkBuilder::new()
			.rpc_urls(vec!["https://rpc1.com", "https://rpc2.com"])
			.build();

		assert_eq!(network.rpc_urls.len(), 2);
		assert_eq!(
			network.rpc_urls[0].url.as_ref().to_string(),
			"https://rpc1.com".to_string()
		);
		assert_eq!(
			network.rpc_urls[1].url.as_ref().to_string(),
			"https://rpc2.com".to_string()
		);
		// Check defaults are applied
		assert!(network.rpc_urls.iter().all(|url| url.type_ == "rpc"));
		assert!(network.rpc_urls.iter().all(|url| url.weight == 100));
	}

	#[test]
	fn test_stellar_network() {
		let network = NetworkBuilder::new()
			.name("Stellar")
			.slug("xlm")
			.network_type(BlockChainType::Stellar)
			.network_passphrase("Test SDF Network")
			.build();

		assert_eq!(network.network_type, BlockChainType::Stellar);
		assert_eq!(
			network.network_passphrase,
			Some("Test SDF Network".to_string())
		);
		assert_eq!(network.chain_id, Some(1)); // From default
	}
}
