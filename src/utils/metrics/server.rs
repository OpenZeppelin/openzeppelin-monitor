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
