# Local EVM Testing Contracts

Minimal Foundry project for local EVM testing with OpenZeppelin Monitor. It provides a simple ERC-20-style token (`TestToken`) that you can deploy to Anvil and monitor for transfers.

## Setup

Install the Forge standard library (if not already present):

```bash
forge install foundry-rs/forge-std
```

## Run Tests

Verify the contracts work before using them with the monitor:

```bash
forge test
```

## Deploy to Anvil

1. Start Anvil in a separate terminal:

   ```bash
   anvil
   ```

2. Deploy the token:

   ```bash
   forge script script/Deploy.s.sol --broadcast --rpc-url http://127.0.0.1:8545
   ```

   The script deploys `TestToken`, mints 1000 tokens (18 decimals) to the deployer, and **prints the token contract address**. Use this address in the monitor configuration (e.g. in `config/monitors/evm_transfer_local.json`).

## Full Instructions

For end-to-end setup (Anvil, monitor config, and triggering a condition), see the [Local EVM Testing](/monitor/local-evm-testing) documentation in the main docs.
