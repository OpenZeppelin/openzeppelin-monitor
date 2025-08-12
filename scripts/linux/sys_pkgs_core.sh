#!/bin/bash

set -euo pipefail

# Colors for pretty printing
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

info() { echo -e "${BLUE}ℹ${NC} $*"; }
success() { echo -e "${GREEN}✅${NC} $*"; }
warn() { echo -e "${YELLOW}⚠${NC} $*"; }
error() { echo -e "${RED}❌${NC} $*"; }
step() { echo -e "${CYAN}➤${NC} $*"; }

# Don't run as root
if [[ $EUID -eq 0 ]]; then
    error "Don't run this as root - it will use sudo when needed"
    exit 1
fi

# Check if we're on Ubuntu/Debian
if ! command -v apt-get >/dev/null; then
    error "This script requires apt-get (Ubuntu/Debian)"
    exit 1
fi

step "Removing problematic cdrom sources..."
sudo sed -i '/cdrom:/d' /etc/apt/sources.list /etc/apt/sources.list.d/*.list 2>/dev/null || true
success "Cleaned up package sources"

# Update with retry
step "Updating package lists..."
for i in {1..3}; do
    if sudo apt-get update >/dev/null 2>&1; then
        success "Package lists updated"
        break
    elif [[ $i -eq 3 ]]; then
        error "Failed to update after 3 attempts"
        exit 1
    else
        warn "Retry $i failed, trying again in 5 seconds..."
        sleep 5
    fi

done

# Install core packages
step "Installing core packages..."
sudo apt-get install -y \
    build-essential \
    curl \
    git \
    pkg-config \
    libssl-dev \
    libffi-dev \
    libyaml-dev \
    software-properties-common \
    ca-certificates >/dev/null 2>&1
success "Core packages installed"

echo -e "\n${GREEN}🎉 Core system setup complete!${NC}"
