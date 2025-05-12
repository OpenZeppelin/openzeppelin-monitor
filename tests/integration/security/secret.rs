use once_cell::sync::Lazy;
use std::sync::Mutex;
use std::{env, fs};
use tempfile::TempDir;
use zeroize::Zeroize;

use openzeppelin_monitor::models::{BlockChainType, SecretString, SecretValue};
use openzeppelin_monitor::repositories::{NetworkRepository, NetworkRepositoryTrait};
use openzeppelin_monitor::utils::tests::builders::network::NetworkBuilder;

// Lock to prevent concurrent test execution
static TEST_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn test_secret_resolution_from_network_config() {
	let _lock = TEST_LOCK.lock().unwrap();

	// Create a temporary directory for our test
	let temp_dir = TempDir::new().unwrap();
	let config_path = temp_dir.path().join("network.json");

	// Set up test environment variables
	const RPC_URL_ENV: &str = "TEST_RPC_URL";
	const RPC_URL_VALUE: &str = "https://test-rpc.example.com";
	env::set_var(RPC_URL_ENV, RPC_URL_VALUE);

	// Create test network configuration using NetworkBuilder
	let network = NetworkBuilder::new()
		.name("Ethereum Testnet")
		.slug("ethereum_testnet")
		.network_type(BlockChainType::EVM)
		.chain_id(1)
		.block_time_ms(12000)
		.confirmation_blocks(12)
		.cron_schedule("0 */1 * * * *")
		.max_past_blocks(50)
		.store_blocks(true)
		.clear_rpc_urls()
		.add_rpc_url("https://eth.drpc.org", "rpc", 100)
		.add_secret_rpc_url(SecretValue::Environment(RPC_URL_ENV.to_string()), "rpc", 90)
		.build();

	// Write config to file
	let config_json = serde_json::to_string_pretty(&network).unwrap();
	fs::write(&config_path, config_json).unwrap();

	// Create a repository instance using `load_all` to call `load_from_path` which resolves secrets
	let repository = NetworkRepository::load_all(Some(temp_dir.path()))
		.await
		.unwrap();

	let loaded_network = repository.get("network").unwrap();

	// Test plain RPC URL resolution
	let plain_rpc = loaded_network.rpc_urls[0].url.resolve().await.unwrap();
	assert_eq!(plain_rpc.as_str(), "https://eth.drpc.org");

	// Test environment variable RPC URL resolution
	let env_rpc = loaded_network.rpc_urls[1].url.resolve().await.unwrap();
	assert_eq!(env_rpc.as_str(), RPC_URL_VALUE);

	// Clean up
	env::remove_var(RPC_URL_ENV);
}

#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn test_secret_zeroization() {
	let _lock = TEST_LOCK.lock().unwrap();

	// Create a secret value
	let mut secret = SecretValue::Plain(SecretString::new("sensitive_data".to_string()));

	// Verify the secret is accessible
	let resolved = secret.resolve().await.unwrap();
	assert_eq!(resolved.as_str(), "sensitive_data");

	// Zeroize the secret
	secret.zeroize();

	// Verify the secret is cleared
	if let SecretValue::Plain(ref secret_string) = secret {
		assert_eq!(secret_string.as_str(), "");
	}
}

#[tokio::test]
async fn test_secret_serialization_deserialization() {
	let _lock = TEST_LOCK.lock().unwrap();

	// Create test secrets
	let plain_secret = SecretValue::Plain(SecretString::new("test_plain".to_string()));
	let env_secret = SecretValue::Environment("TEST_ENV_VAR".to_string());
	let vault_secret = SecretValue::HashicorpCloudVault("test-vault-secret".to_string());

	// Serialize to JSON
	let plain_json = serde_json::to_string(&plain_secret).unwrap();
	let env_json = serde_json::to_string(&env_secret).unwrap();
	let vault_json = serde_json::to_string(&vault_secret).unwrap();

	// Deserialize back
	let deserialized_plain: SecretValue = serde_json::from_str(&plain_json).unwrap();
	let deserialized_env: SecretValue = serde_json::from_str(&env_json).unwrap();
	let deserialized_vault: SecretValue = serde_json::from_str(&vault_json).unwrap();

	// Verify the deserialized values match
	assert_eq!(deserialized_plain, plain_secret);
	assert_eq!(deserialized_env, env_secret);
	assert_eq!(deserialized_vault, vault_secret);
}
