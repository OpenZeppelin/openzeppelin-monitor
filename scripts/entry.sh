#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# Copy configs
"$SCRIPT_DIR/setup.sh"

# Start docker
"$SCRIPT_DIR/docker_compose.sh" up
