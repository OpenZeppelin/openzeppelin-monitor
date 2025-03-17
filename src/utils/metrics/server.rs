//! Metrics server module
//!
//! This module provides an HTTP server to expose Prometheus metrics for scraping.

use actix_web::middleware::{Compress, DefaultHeaders, NormalizePath};
use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use log::{error, info};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::{
	repositories::{
		MonitorRepository, MonitorRepositoryTrait, NetworkRepository, NetworkRepositoryTrait,
		TriggerRepository, TriggerRepositoryTrait,
	},
	utils::metrics::{gather_metrics, update_monitoring_metrics, update_system_metrics},
};

/// Metrics endpoint handler
async fn metrics_handler(
	monitor_repo: web::Data<Arc<Mutex<MonitorRepository<NetworkRepository, TriggerRepository>>>>,
	network_repo: web::Data<Arc<Mutex<NetworkRepository>>>,
	trigger_repo: web::Data<Arc<Mutex<TriggerRepository>>>,
) -> impl Responder {
	// Update system metrics
	update_system_metrics();

	// Get current state and update metrics
	{
		let monitors = monitor_repo.lock().await.get_all();
		let networks = network_repo.lock().await.get_all();
		let triggers = trigger_repo.lock().await.get_all();

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
	monitor_repo: Arc<Mutex<MonitorRepository<NetworkRepository, TriggerRepository>>>,
	network_repo: Arc<Mutex<NetworkRepository>>,
	trigger_repo: Arc<Mutex<TriggerRepository>>,
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
			.app_data(web::Data::new(Arc::clone(&monitor_repo)))
			.app_data(web::Data::new(Arc::clone(&network_repo)))
			.app_data(web::Data::new(Arc::clone(&trigger_repo)))
			.route("/metrics", web::get().to(metrics_handler))
	})
	.workers(2)
	.bind(actual_bind_address)?
	.shutdown_timeout(5)
	.run())
}
