//! Metrics server module
//!
//! This module provides an HTTP server to expose Prometheus metrics for scraping.

use actix_web::middleware::{Compress, DefaultHeaders, NormalizePath};
use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info};

use crate::{
	repositories::{
		MonitorRepository, MonitorService, NetworkRepository, NetworkService, TriggerRepository,
		TriggerService,
	},
	utils::metrics::{gather_metrics, update_monitoring_metrics, update_system_metrics},
};

// Type aliases to simplify complex types in function signatures
//  MonitorService
pub type MonitorServiceData = web::Data<
	Arc<
		Mutex<
			MonitorService<
				MonitorRepository<NetworkRepository, TriggerRepository>,
				NetworkRepository,
				TriggerRepository,
			>,
		>,
	>,
>;

// NetworkService
pub type NetworkServiceData = web::Data<Arc<Mutex<NetworkService<NetworkRepository>>>>;

// TriggerService
pub type TriggerServiceData = web::Data<Arc<Mutex<TriggerService<TriggerRepository>>>>;

// For Arc<Mutex<...>> MonitorService
pub type MonitorServiceArc = Arc<
	Mutex<
		MonitorService<
			MonitorRepository<NetworkRepository, TriggerRepository>,
			NetworkRepository,
			TriggerRepository,
		>,
	>,
>;

// For Arc<Mutex<...>> NetworkService
pub type NetworkServiceArc = Arc<Mutex<NetworkService<NetworkRepository>>>;

// For Arc<Mutex<...>> TriggerService
pub type TriggerServiceArc = Arc<Mutex<TriggerService<TriggerRepository>>>;

/// Metrics endpoint handler
async fn metrics_handler(
	monitor_service: MonitorServiceData,
	network_service: NetworkServiceData,
	trigger_service: TriggerServiceData,
) -> impl Responder {
	// Update system metrics
	update_system_metrics();

	// Get current state and update metrics
	{
		let monitors = monitor_service.lock().await.get_all();
		let networks = network_service.lock().await.get_all();
		let triggers = trigger_service.lock().await.get_all();

		update_monitoring_metrics(&monitors, &triggers, &networks);
	}

	// Gather all metrics
	match gather_metrics() {
		Ok(buffer) => HttpResponse::Ok()
			.content_type("text/plain; version=0.0.4; charset=utf-8")
			.body(buffer),
		Err(e) => {
			error!("Error gathering metrics: {}", e);
			HttpResponse::InternalServerError().finish()
		}
	}
}

// Create metrics server
pub fn create_metrics_server(
	bind_address: String,
	monitor_service: MonitorServiceArc,
	network_service: NetworkServiceArc,
	trigger_service: TriggerServiceArc,
) -> std::io::Result<actix_web::dev::Server> {
	let actual_bind_address = if std::env::var("IN_DOCKER").unwrap_or_default() == "true" {
		if let Some(port) = bind_address.split(':').nth(1) {
			format!("0.0.0.0:{}", port)
		} else {
			"0.0.0.0:8081".to_string()
		}
	} else {
		bind_address.clone()
	};

	info!(
		"Starting metrics server on {} (actual bind: {})",
		bind_address, actual_bind_address
	);

	Ok(HttpServer::new(move || {
		App::new()
			.wrap(Compress::default())
			.wrap(NormalizePath::trim())
			.wrap(DefaultHeaders::new())
			.app_data(web::Data::new(monitor_service.clone()))
			.app_data(web::Data::new(network_service.clone()))
			.app_data(web::Data::new(trigger_service.clone()))
			.route("/metrics", web::get().to(metrics_handler))
	})
	.workers(2)
	.bind(actual_bind_address)?
	.shutdown_timeout(5)
	.run())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		models::{BlockChainType, Monitor, Network, Trigger},
		repositories::{
			MonitorService, NetworkRepository, NetworkService, TriggerRepository, TriggerService,
		},
		utils::tests::{
			evm::monitor::MonitorBuilder, network::NetworkBuilder, trigger::TriggerBuilder,
		},
	};
	use actix_web::{test, App};
	use std::{fs, path::PathBuf};
	use tempfile::TempDir;
	use tokio::net::TcpListener;

	fn create_test_monitor(
		name: &str,
		networks: Vec<&str>,
		paused: bool,
		triggers: Vec<&str>,
	) -> Monitor {
		MonitorBuilder::new()
			.name(name)
			.networks(networks.into_iter().map(|s| s.to_string()).collect())
			.paused(paused)
			.triggers(triggers.into_iter().map(|s| s.to_string()).collect())
			.build()
	}

	fn create_test_trigger(name: &str) -> Trigger {
		TriggerBuilder::new()
			.name(name)
			.slack("https://hooks.slack.com/services/T00000000/B00000000/XXXXXXXXXXXXXXXXXXXXXXXX") //noboost
			.message("Test Title", "Test Body")
			.build()
	}

	pub fn create_test_network(name: &str, slug: &str, network_type: BlockChainType) -> Network {
		NetworkBuilder::new()
			.name(name)
			.slug(slug)
			.network_type(network_type)
			.chain_id(1)
			.rpc_url("http://localhost:8545")
			.block_time_ms(1000)
			.confirmation_blocks(1)
			.cron_schedule("*/5 * * * * *")
			.store_blocks(false)
			.build()
	}

	fn create_mock_configs() -> (PathBuf, PathBuf, PathBuf, TempDir) {
		// Create a temporary directory
		let temp_dir = TempDir::new().expect("Failed to create temporary directory");
		let config_path = temp_dir.path().join("config");
		let monitor_dir = config_path.join("monitors");
		let trigger_dir = config_path.join("triggers");
		let network_dir = config_path.join("networks");

		// Create directories
		fs::create_dir_all(&monitor_dir).expect("Failed to create monitor directory");
		fs::create_dir_all(&trigger_dir).expect("Failed to create trigger directory");
		fs::create_dir_all(&network_dir).expect("Failed to create network directory");

		let monitor_path = monitor_dir.join("test_monitor.json");
		let trigger_path = trigger_dir.join("test_trigger.json");
		let network_path = network_dir.join("test_network.json");

		fs::write(
			&monitor_path,
			serde_json::to_string(&create_test_monitor(
				"test_monitor",
				vec!["ethereum_mainnet"],
				false,
				vec!["test_trigger"],
			))
			.unwrap(),
		)
		.expect("Failed to create mock monitor");

		fs::write(
			&trigger_path,
			serde_json::to_string(&create_test_trigger("test_trigger")).unwrap(),
		)
		.expect("Failed to create mock trigger");

		fs::write(
			&network_path,
			serde_json::to_string(&create_test_network(
				"Ethereum Mainnet",
				"ethereum_mainnet",
				BlockChainType::EVM,
			))
			.unwrap(),
		)
		.expect("Failed to create mock network");

		// Return directory paths and temp_dir to keep it alive
		(monitor_dir, trigger_dir, network_dir, temp_dir)
	}

	async fn create_test_services() -> (
		MonitorServiceArc,
		NetworkServiceArc,
		TriggerServiceArc,
		TempDir,
	) {
		let (monitor_path, trigger_path, network_path, temp_dir) = create_mock_configs();
		let network_service =
			NetworkService::<NetworkRepository>::new(Some(network_path.parent().unwrap()))
				.await
				.unwrap();
		let trigger_service =
			TriggerService::<TriggerRepository>::new(Some(trigger_path.parent().unwrap()))
				.await
				.unwrap();
		let monitor_service = MonitorService::new(
			Some(monitor_path.parent().unwrap()),
			Some(network_service.clone()),
			Some(trigger_service.clone()),
		)
		.await
		.unwrap();

		(
			Arc::new(Mutex::new(monitor_service)),
			Arc::new(Mutex::new(network_service)),
			Arc::new(Mutex::new(trigger_service)),
			temp_dir,
		)
	}

	#[actix_web::test]
	async fn test_metrics_handler() {
		// Create test services
		let (monitor_service, network_service, trigger_service, _temp_dir) =
			create_test_services().await;

		// Create test app
		let app = test::init_service(
			App::new()
				.app_data(web::Data::new(monitor_service.clone()))
				.app_data(web::Data::new(network_service.clone()))
				.app_data(web::Data::new(trigger_service.clone()))
				.route("/metrics", web::get().to(metrics_handler)),
		)
		.await;

		// Create test request
		let req = test::TestRequest::get().uri("/metrics").to_request();

		// Execute request
		let resp = test::call_service(&app, req).await;

		// Assert response is successful
		assert!(resp.status().is_success());

		// Check content type
		let content_type = resp
			.headers()
			.get("content-type")
			.unwrap()
			.to_str()
			.unwrap();
		assert_eq!(content_type, "text/plain; version=0.0.4; charset=utf-8");

		// Verify response body contains expected metrics
		let body = test::read_body(resp).await;
		let body_str = String::from_utf8(body.to_vec()).unwrap();

		// Basic check that we have some metrics content
		assert!(body_str.contains("# HELP"));
	}

	#[tokio::test]
	async fn test_create_metrics_server() {
		// Create test services
		let (monitor_service, network_service, trigger_service, _temp_dir) =
			create_test_services().await;

		// Find an available port
		let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
		let port = listener.local_addr().unwrap().port();
		drop(listener);

		let bind_address = format!("127.0.0.1:{}", port);

		// Create server
		let server = create_metrics_server(
			bind_address.clone(),
			monitor_service,
			network_service,
			trigger_service,
		);

		// Assert server creation is successful
		assert!(server.is_ok());

		// Start server in a separate thread so it can be dropped immediately
		let server_handle = server.unwrap();
		let server_task = tokio::spawn(async move {
			// This will run until the server is stopped
			let result = server_handle.await;
			assert!(result.is_ok(), "Server should shut down gracefully");
		});

		// Give the server a moment to start
		tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

		// Make a request to verify the server is actually running
		let client = reqwest::Client::new();
		let response = client
			.get(format!("http://{}/metrics", bind_address))
			.timeout(std::time::Duration::from_secs(1))
			.send()
			.await;

		// Verify we got a successful response
		assert!(response.is_ok(), "Server should respond to requests");
		let response = response.unwrap();
		assert!(
			response.status().is_success(),
			"Server should return 200 OK"
		);

		// Gracefully shut down the server
		server_task.abort();
	}

	#[tokio::test]
	async fn test_docker_bind_address_handling() {
		// Save original environment state
		let original_docker_env = std::env::var("IN_DOCKER").ok();

		// Set IN_DOCKER environment variable
		std::env::set_var("IN_DOCKER", "true");

		// Mock the HttpServer::bind function to avoid actual network binding
		// We'll just test the address transformation logic
		let bind_address = "localhost:8081".to_string();
		let actual_bind_address = if std::env::var("IN_DOCKER").unwrap_or_default() == "true" {
			if let Some(port) = bind_address.split(':').nth(1) {
				format!("0.0.0.0:{}", port)
			} else {
				"0.0.0.0:8081".to_string()
			}
		} else {
			bind_address.clone()
		};

		// Verify the address transformation logic
		assert_eq!(actual_bind_address, "0.0.0.0:8081");

		// Test with no port specified
		let bind_address = "localhost".to_string();
		let actual_bind_address = if std::env::var("IN_DOCKER").unwrap_or_default() == "true" {
			if let Some(port) = bind_address.split(':').nth(1) {
				format!("0.0.0.0:{}", port)
			} else {
				"0.0.0.0:8081".to_string()
			}
		} else {
			bind_address.clone()
		};

		// Verify the address transformation logic
		assert_eq!(actual_bind_address, "0.0.0.0:8081");

		// Restore original environment
		match original_docker_env {
			Some(val) => std::env::set_var("IN_DOCKER", val),
			None => std::env::remove_var("IN_DOCKER"),
		}
	}
}
