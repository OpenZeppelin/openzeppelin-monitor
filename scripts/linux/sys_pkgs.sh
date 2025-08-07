#!/bin/bash

set -e

echo "Setting up development environment from clean slate (Ubuntu)"

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
