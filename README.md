# OpenZeppelin Monitor

A blockchain monitoring service that watches for specific on-chain activities and triggers notifications based on configurable conditions. The service offers multi-chain support with configurable monitoring schedules, flexible trigger conditions, and an extensible architecture for adding new chains.

## Architecture

```mermaid
%%{init: {
    'theme': 'base',
    'themeVariables': {
        'background': '#ffffff',
        'mainBkg': '#ffffff',
        'primaryBorderColor': '#cccccc'
    }
}}%%
graph TD
    subgraph Blockchain Networks
        ETH[Ethereum RPC]
        POL[Polygon RPC]
        BSC[BSC RPC]
    end

    subgraph Block Processing
        BW[BlockWatcherService]
        BS[(BlockStorage)]
        JS[JobScheduler]
    end

    subgraph Client Layer
        BC[BlockchainClient]
        EVM[EVMClient]
        STL[StellarClient]
    end

    subgraph Processing Pipeline
        FS[FilterService]
        TS[TriggerService]
        NS[NotificationService]
    end

    subgraph Notifications
        Slack
        Email
    end

    %% Block Processing Flow
    JS -->|Schedule Block Fetch| BW
    BW -->|Store Last Block| BS
    BW -->|Read Last Block| BS
    BW -->|Get New Blocks| BC

    %% Client Connections
    BC --> EVM
    BC --> STL
    EVM -->|RPC Calls| ETH
    EVM -->|RPC Calls| POL
    EVM -->|RPC Calls| BSC

    %% Processing Flow
    BW -->|New Block| FS
    FS -->|Matches| TS
    TS -->|Execute| NS
    NS --> Slack
    NS --> Email

    style STL fill:#f0f0f0

    classDef rpc fill:#e1f5fe,stroke:#01579b
    classDef storage fill:#fff3e0,stroke:#ef6c00
    classDef service fill:#e8f5e9,stroke:#2e7d32
    classDef notification fill:#f3e5f5,stroke:#7b1fa2

    class ETH,POL,BSC rpc
    class BS storage
    class BW,FS,TS,NS service
    class Slack,Email notification
```

## Supported Networks

- EVM
- Stellar

## Supported Triggers

- Slack notifications
- Email notifications

## Prerequisites

- Rust 2021 edition

## Installation

### Local Setup

1. Clone the repository:

```bash
git clone https://github.com/openzeppelin/openzeppelin-monitor
cd openzeppelin-monitor
```

2. Install dependencies:

```bash
cargo build
```

## Configuration

1. Configure your environment variables in `.env` file.

```bash
cp .env.example .env
```

2. Copy example configuration files:

```bash
# EVM
cp config/monitors/evm_transfer_usdc.json.example config/monitors/evm_transfer_usdc.json
cp config/networks/ethereum_mainnet.json.example config/networks/ethereum_mainnet.json
cp config/triggers/email_notifications.json.example config/triggers/email_notifications.json
cp config/triggers/slack_notifications.json.example config/triggers/slack_notifications.json

# Stellar
cp config/monitors/stellar_transfer_usdc.json.example config/monitors/stellar_transfer_usdc.json
cp config/networks/stellar_mainnet.json.example config/networks/stellar_mainnet.json
cp config/triggers/email_notifications.json.example config/triggers/email_notifications.json
cp config/triggers/slack_notifications.json.example config/triggers/slack_notifications.json
```

3. Configure your networks in `config/networks/`:

- EVM networks: See `evm_mainnet.json`
- Stellar networks: See `stellar_mainnet.json`

4. Configure your monitors in `config/monitors/`:

- EVM monitors: See `evm_transfer_usdc.json`
- Stellar monitors: See `stellar_transfer_usdc.json`

5. Configure your triggers in `config/triggers/`:

- Slack notifications: See `slack_notifications.json`
- Email notifications: See `email_notifications.json`

### Monitor Argument Access

- **Stellar**: Arguments are accessed by numeric index:

  - For function `transfer(Address,Address,I128)`
    - Access via [0, 1, 2]
    - For example: `"expression": "2 > 1000"`

- **EVM**: Arguments are accessed by parameter names defined in the ABI:

  - For event `Transfer(address from, address to, uint256 value)`
    - Access via ["from", "to", "value"]
    - For example: `"expression": "value > 10000000000"`

### Condition Evaluation Rules

- No conditions → All transactions match
- Transaction-only conditions → Only transaction properties are checked
- Event/function conditions → Either event or function matches trigger
- Both transaction and event/function conditions → Both must be satisfied

### Template Variables

Template variables may be used to inject specific values related to the monitor match into the body of the notification.

- **Common Variables**:

  - `monitor_name`: Name of the triggered monitor
  - `transaction_hash`: Hash of the transaction
  - `function_[index]_signature`: Signature of the function (e.g. `function_0_signature`)
  - `event_[index]_signature`: Signature of the event (e.g. `event_0_signature`)

- **EVM-specific Variables**:

  - `transaction_from`: Sender address
  - `transaction_to`: Recipient address
  - `transaction_value`: Transaction value
  - `event_[index]_[param]`: Event parameters (e.g., `event_0_value`)
  - `function_[index]_[param]`: Function parameters (e.g., `function_0_amount`)

- **Stellar-specific Variables**:
  - `event_[index]_[position]`: Event parameters by position (e.g., `event_0_0`)
  - `function_[index]_[position]`: Function parameters by position (e.g., `function_0_2`)
  - Note: `transaction_from`, `transaction_to`, and `transaction_value` are not available

Example usage in trigger body:

For EVM:
`"body": "Transfer of ${event_0_value} from ${transaction_from}"`

For Stellar:
`"body": "Transfer of ${function_0_2} from account ${function_0_0}"`

## Usage

### Run Locally

```bash
cargo run
```

### Run as a container

You have the option of running as a development container or as production one (`Dockerfile.development` and `Dockerfile.production`).

To adjust the `.env` file, you can edit `env_dev` or `env_prod` at the root of the repository.

Assuming your docker environment is installed and properly set up, you can build the image with:
```bash
docker build --tag <your_image_tag> -f Dockerfile.<development | production> --squash-all
```

This will generate an image including the appropriate `.env` file and the configurations in the `./config` folder.

Now we need to create the docker volume to keep the monitor internal data (if you want to keep it when you restart the container, otherwise just skip this step).
```bash
docker volume create <volume_tag>
```

After the build is finished, you can run it with:
```bash
docker run --volume <volume_tag>:/app/data <your_image_tag>`
```

If you need to change the monitor configurations, there's no need to rebuild the image. Assuming you have the configuration files in `./config`, you can just bind the directory to the container:
```bash
docker run --mount type=bind,src=./config,dst=/app/config,ro --volume <volume_tag>:/app/data <your_image_tag>
```

### Run Tests

```bash
cargo test
cargo test properties
cargo test integration
```

### Developer setup

1. Run `chmod +x .githooks/*` to make the git hooks executable.
2. Run `git config core.hooksPath .githooks` to setup git hooks for linting and formatting.
3. Run `rustup toolchain install nightly` to install the nightly toolchain.
4. Run `rustup component add rustfmt --toolchain nightly` to install rustfmt for the nightly toolchain.

## Project Structure

- `src/`: Source code
  - `models/`: Data structures and types
  - `repositories/`: Configuration storage
  - `services/`: Core business logic
  - `utils/`: Helper functions
- `config/`: Configuration files
- `tests/`: Integration tests
- `data/`: Runtime data storage

## Caveats

- This software is in alpha. Use in production environments at your own risk.
- EVM monitors require an ABI to decode and trigger on contract events and functions.
- Monitor performance depends on network congestion and RPC endpoint reliability.
- The `max_past_blocks` configuration is critical to prevent missing blocks:
  - Should be approximately calculated as: `(cron_interval_ms/block_time_ms) + confirmation_blocks + 1` (defaults to this value if not set)
  - Example: For 1-minute cron on Ethereum (~12s blocks, 12 confirmation blocks):
    - `(60000/12000) + 12 + 1 = 18 blocks`
  - Setting this too low may result in missed blocks, especially on fast networks
  - Consider network congestion and block time variability when configuring
- For email notifications, the `port` field is optional and defaults to 465.
- Template variable availability depends on the trigger source:
  - If triggered by an event, only event variables will be populated.
  - If triggered by a function, only function variables will be populated.
  - Using event variables in a function-triggered notification (or vice versa) will result in empty values.

## Contributing

1. Fork the repository
2. Create your feature branch
3. Commit your changes
4. Push to the branch
5. Create a Pull Request

Please read our [Code of Conduct](CODE_OF_CONDUCT.md) and check the [Security Policy](SECURITY.md) for reporting vulnerabilities.

## License

This project is licensed under the GNU Affero General Public License v3.0 - see the [LICENSE](LICENSE) file for details.

## Security

For security concerns, please refer to our [Security Policy](SECURITY.md).

## Contact

For support or inquiries, contact defender-support@openzeppelin.com.

## Maintainers

See [CODEOWNERS](CODEOWNERS) file for the list of project maintainers.
