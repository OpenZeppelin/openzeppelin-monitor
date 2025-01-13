//! Integration tests for the OpenZeppelin Monitor.
//!
//! Contains tests for blockchain monitoring functionality across different
//! chains (EVM and Stellar) and mock implementations for testing.

pub mod mocks;

mod properties {
    mod monitor_prop_tests;
}

mod filter {
    mod common;
    mod evm;
    mod stellar;
}
