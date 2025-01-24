//! Blockchain monitoring and notification service.
//!
//! This library provides functionality for monitoring blockchain networks and triggering
//! notifications based on configurable conditions. It includes:
//!
//! - Configuration management through JSON files
//! - Blockchain network monitoring and event filtering
//! - Customizable notification triggers and actions
//! - Extensible repository and service architecture
//!
//! # Module Structure
//!
//! - `bootstrap`: Bootstraps the application
//! - `models`: Data structures for configuration and blockchain data
//! - `repositories`: Configuration storage and management
//! - `services`: Core business logic and blockchain interaction
//! - `utils`: Common utilities and helper functions

pub mod bootstrap;
pub mod models;
pub mod repositories;
pub mod services;
pub mod utils;
