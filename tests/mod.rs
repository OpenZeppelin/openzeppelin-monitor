//! Integration and PBT tests for the OpenZeppelin Monitor.
//!
//! Contains tests for blockchain monitoring functionality across different
//! chains (EVM and Stellar) and mock implementations for testing.

mod properties {
    mod monitor;
}

mod integration {
    mod mocks;
    mod filter {
        mod common;
        mod evm;
        mod stellar;
    }
}
