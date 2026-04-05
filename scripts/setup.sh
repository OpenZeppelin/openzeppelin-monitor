#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "Setting up RISE monitor configs..."

# Copy RISE network config (always overwrite since it's RISE-specific)
cp "$ROOT_DIR/examples/config/networks/rise_mainnet.json" "$ROOT_DIR/config/networks/"
echo "  Copied rise_mainnet network config"

# Copy shared network configs only if they don't already exist
for network in ethereum_mainnet base arbitrum_one; do
  dest="$ROOT_DIR/config/networks/${network}.json"
  if [ ! -f "$dest" ]; then
    cp "$ROOT_DIR/examples/config/networks/${network}.json" "$dest"
    echo "  Copied ${network} network config"
  else
    echo "  Skipped ${network} (already exists)"
  fi
done

# Copy monitor configs
cp "$ROOT_DIR/examples/config/monitors/rise_"*.json "$ROOT_DIR/config/monitors/"
cp "$ROOT_DIR/examples/config/monitors/risex_"*.json "$ROOT_DIR/config/monitors/"
echo "  Copied monitor configs"

# Copy trigger configs
cp "$ROOT_DIR/examples/config/triggers/rise_slack_notifications.json" "$ROOT_DIR/config/triggers/"
echo "  Copied trigger configs"

# Verify .env
if [ ! -f "$ROOT_DIR/.env" ]; then
  echo "ERROR: .env not found. Copy .env.example and set required env vars"
  exit 1
fi

# Check required env vars
missing=()
for var in SLACK_WEBHOOK_URL RISE_RPC_URL_1; do
  if ! grep -q "^${var}=" "$ROOT_DIR/.env"; then
    missing+=("$var")
  fi
done

# Check shared network RPC vars only if we copied those configs
for network_prefix in ETH BASE ARB; do
  var="${network_prefix}_RPC_URL_1"
  if ! grep -q "^${var}=" "$ROOT_DIR/.env"; then
    missing+=("$var")
  fi
done

if [ ${#missing[@]} -gt 0 ]; then
  echo "WARNING: Missing env vars in .env: ${missing[*]}"
  echo "  The monitor may fail to start without these."
fi

echo "Setup complete. Run: ./scripts/docker_compose.sh up"
