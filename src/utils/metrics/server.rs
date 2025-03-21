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
	use crate::repositories::{
		MonitorService, NetworkRepository, NetworkService, TriggerRepository, TriggerService,
	};
	use actix_web::{test, App};
	use tokio::net::TcpListener;

	// Helper function to create test services with mock repositories
	fn create_test_services() -> (MonitorServiceArc, NetworkServiceArc, TriggerServiceArc) {
		let network_service = NetworkService::<NetworkRepository>::new(None).unwrap();
		let trigger_service = TriggerService::<TriggerRepository>::new(None).unwrap();
		let monitor_service = MonitorService::new(
			None,
			Some(network_service.clone()),
			Some(trigger_service.clone()),
		)
		.unwrap();

		(
			Arc::new(Mutex::new(monitor_service)),
			Arc::new(Mutex::new(network_service)),
			Arc::new(Mutex::new(trigger_service)),
		)
	}

	#[actix_web::test]
	async fn test_metrics_handler() {
		// Create test services
		let (monitor_service, network_service, trigger_service) = create_test_services();

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
		let (monitor_service, network_service, trigger_service) = create_test_services();

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
