//! Block watcher service implementation.
//!
//! This module provides functionality to watch and process blockchain blocks across
//! different networks. It includes:
//! - Block watching service for multiple networks
//! - Block storage implementations
//! - Error handling specific to block watching operations
//! - Missed block recovery functionality

mod error;
mod recovery;
mod service;
mod storage;
mod tracker;

pub use error::BlockWatcherError;
pub use recovery::{process_missed_blocks, RecoveryResult};
pub use service::{
	process_new_blocks, BlockWatcherService, JobSchedulerTrait, NetworkBlockWatcher,
};
pub use storage::{BlockStorage, FileBlockStorage, MissedBlockEntry, MissedBlockStatus};
pub use tracker::{BlockCheckResult, BlockTracker, BlockTrackerTrait};
