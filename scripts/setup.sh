#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "Setting up RISE monitor configs..."

# Copy network configs
cp "$ROOT_DIR/examples/config/networks/rise_mainnet.json" "$ROOT_DIR/config/networks/"
cp "$ROOT_DIR/examples/config/networks/ethereum_mainnet.json" "$ROOT_DIR/config/networks/"
cp "$ROOT_DIR/examples/config/networks/base.json" "$ROOT_DIR/config/networks/"
cp "$ROOT_DIR/examples/config/networks/arbitrum_one.json" "$ROOT_DIR/config/networks/"
echo "  Copied 4 network configs"

# Copy monitor configs
cp "$ROOT_DIR/examples/config/monitors/rise_"*.json "$ROOT_DIR/config/monitors/"
cp "$ROOT_DIR/examples/config/monitors/risex_"*.json "$ROOT_DIR/config/monitors/"
echo "  Copied monitor configs"

# Copy trigger configs
cp "$ROOT_DIR/examples/config/triggers/rise_slack_notifications.json" "$ROOT_DIR/config/triggers/"
echo "  Copied trigger configs"

# Verify .env
if [ ! -f "$ROOT_DIR/.env" ]; then
  echo "WARNING: .env not found. Copy .env.example and set SLACK_WEBHOOK_URL"
  exit 1
fi

if ! grep -q "SLACK_WEBHOOK_URL" "$ROOT_DIR/.env"; then
  echo "WARNING: SLACK_WEBHOOK_URL not set in .env"
  exit 1
fi

echo "Setup complete. Run: ./scripts/docker_compose.sh up"
