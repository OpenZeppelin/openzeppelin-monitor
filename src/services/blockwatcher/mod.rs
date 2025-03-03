//! Block watcher service implementation.
//!
//! This module provides functionality to watch and process blockchain blocks across
//! different networks. It includes:
//! - Block watching service for multiple networks
//! - Block storage implementations
//! - Error handling specific to block watching operations

mod error;
mod service;
mod storage;
mod tracker;

pub use error::BlockWatcherError;
pub use service::{
	process_new_blocks, BlockWatcherService, JobSchedulerTrait, NetworkBlockWatcher,
};
pub use storage::{BlockStorage, FileBlockStorage};
pub use tracker::{BlockTracker, BlockTrackerTrait};
