#!/bin/bash

set -e

echo "Setting up development environment from clean slate (Ubuntu)"

# Some fresh Ubuntu installs from ISO (especially x86) add a "cdrom:" package source
# to /etc/apt/sources.list. This source fails in VMs without the ISO mounted, causing
# apt update/install errors and missing packages (e.g., linker 'cc' not found).
# Remove it to ensure setup works consistently across architectures and install types.
sudo sed -i '\|cdrom|d' /etc/apt/sources.list /etc/apt/sources.list.d/*.list 2>/dev/null

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
    python3-pip \
    software-properties-common

# Ensure Python 3.9+ is available for pre-commit compatibility
echo "Checking Python version for pre-commit compatibility..."
python_version=$(python3 -c 'import sys; print(f"{sys.version_info.major}.{sys.version_info.minor}")' 2>/dev/null || echo "0.0")

if python3 -c "import sys; exit(0 if sys.version_info >= (3, 9) else 1)" 2>/dev/null; then
    echo "✅ Python $python_version is compatible with pre-commit hooks"
else
    echo "⚠️  Python $python_version detected - installing Python 3.11 for better pre-commit support..."
    
    # Add deadsnakes PPA for newer Python versions (if not already added)
    if ! grep -q "deadsnakes/ppa" /etc/apt/sources.list.d/* 2>/dev/null; then
        sudo add-apt-repository -y ppa:deadsnakes/ppa
        sudo apt update
    fi
    
    # Install Python 3.11 and related packages
    sudo apt install -y python3.11 python3.11-venv python3.11-dev python3.11-distutils
    
    # Install pip for Python 3.11
    if ! python3.11 -m pip --version >/dev/null 2>&1; then
        curl -sS https://bootstrap.pypa.io/get-pip.py | sudo python3.11
    fi
    
    echo "✅ Python 3.11 installed successfully!"
    echo "💡 For pre-commit setup, you can use either:"
    echo "   - python3.11 -m pip install pre-commit"
    echo "   - Or set python3.11 as default: sudo update-alternatives --install /usr/bin/python3 python3 /usr/bin/python3.11 1"
fi

echo "✅ System packages installed successfully!"
