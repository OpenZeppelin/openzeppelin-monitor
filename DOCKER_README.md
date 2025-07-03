# OpenZeppelin Monitor

> ⚠️ This software is in alpha. Use in production environments at your own risk.

In the rapidly evolving world of blockchain technology, effective monitoring is crucial for ensuring security and performance. OpenZeppelin Monitor is a blockchain monitoring service that watches for specific on-chain activities and triggers notifications based on configurable conditions. The service offers multi-chain support with configurable monitoring schedules, flexible trigger conditions, and an extensible architecture for adding new chains.

[Install](https://docs.openzeppelin.com/monitor#getting_started) | [User Docs](https://docs.openzeppelin.com/monitor) | [Quickstart](https://docs.openzeppelin.com/monitor/quickstart) | [Crate Docs](https://docs.openzeppelin.com/monitor/rust_docs/doc/openzeppelin_monitor/)

## Pre-requisites

- Docker installed on your machine
- Copy example configuration files to `./config` directory and modify according to your needs. See [examples](https://docs.openzeppelin.com/monitor/quickstart#examples) for more information.

## How to use images pushed to DockerHub

- These images are automatically pulled when you use docker compose. See [using docker compose](https://docs.openzeppelin.com/monitor#run_with_docker) for more information.
- If you are not using docker compose and you want to use these images, follow the steps below.

### 1. Pull the image

You can pull the latest image using the following command:

```bash
docker pull openzeppelin/openzeppelin-monitor:latest
```

### 2. Run the image

You can run the image using the following command:

```bash
docker run -d \
  --name monitor \
  -v ./config:/app/config:ro \
  openzeppelin/openzeppelin-monitor:latest
```

### 3. Stop the container

You can stop the container using the following command:

```bash
docker stop monitor
```

### 4. Remove the container

You can remove the container using the following command:

```bash
docker rm monitor
```

### 5. Remove the image

You can remove the image using the following command:

```bash
docker rmi openzeppelin/openzeppelin-monitor:latest
```

## Contributing

We welcome contributions from the community. Please read our [contributing section](https://github.com/OpenZeppelin/openzeppelin-monitor/?tab=readme-ov-file#contributing) for more information.

## License

This project is licensed under the GNU Affero General Public License v3.0 - see the [LICENSE](https://github.com/OpenZeppelin/openzeppelin-monitor/blob/main/LICENSE) file for details.

## Security

For security concerns, please refer to our [Security Policy](https://github.com/OpenZeppelin/openzeppelin-monitor/blob/main/SECURITY.md).

## Get Help

If you have any questions, first see if the answer to your question can be found in the [User Documentation](https://docs.openzeppelin.com/monitor).

If the answer is not there:

- Join the [Telegram](https://t.me/openzeppelin_tg/4) to get help, or
- Open an issue with [the bug](https://github.com/openzeppelin/openzeppelin-monitor/issues/new?assignees=&labels=T-bug%2CS-needs-triage&projects=&template=bug.yml)

We encourage you to reach out with any questions or feedback.
