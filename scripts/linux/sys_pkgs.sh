#!/bin/bash

set -e

echo "Setting up development environment from clean slate (Ubuntu)"

# Check if running on Ubuntu/Debian
if ! command -v apt >/dev/null 2>&1; then
    echo "Error: This script is for Ubuntu/Debian systems with apt package manager"
    exit 1
fi

echo "Installing required system packages..."
sudo apt update
sudo apt install -y \
    build-essential \
    curl \
    git \
    pkg-config \
    libssl-dev \
    libffi-dev \
    libyaml-dev \
    python3 \
    python3-venv \
    python3-pip

echo "✅ System packages installed successfully!"
