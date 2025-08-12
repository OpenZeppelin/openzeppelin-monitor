#!/bin/bash

set -euo pipefail

# Colors for pretty printing
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Pretty print functions
info() {
    echo -e "${BLUE}ℹ${NC} $*"
}

success() {
    echo -e "${GREEN}✅${NC} $*"
}

warn() {
    echo -e "${YELLOW}⚠${NC} $*"
}

error() {
    echo -e "${RED}❌${NC} $*"
}

step() {
    echo -e "${CYAN}➤${NC} $*"
}

echo -e "${GREEN}🚀 Setting up development environment...${NC}\n"

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

# Install packages
step "Installing core packages..."
sudo apt-get install -y \
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
    software-properties-common \
    ca-certificates >/dev/null 2>&1

success "Core packages installed"

# Check Python version
step "Checking Python compatibility..."
python_version=$(python3 -c 'import sys; print(f"{sys.version_info.major}.{sys.version_info.minor}")' 2>/dev/null || echo "unknown")
info "Found Python $python_version"

python_ok=true
if ! python3 -c "import sys; exit(0 if sys.version_info >= (3, 9) else 1)" 2>/dev/null; then
    python_ok=false
fi

# Install newer Python if needed
if [[ "$python_ok" == false ]]; then
    warn "Python $python_version is below recommended 3.9+"
    step "Installing Python 3.11 for better compatibility..."
    warn "You are about to add a 3rd-party PPA (deadsnakes) for newer Python versions."
    read -p "Continue? [y/N]: " confirm
    if [[ ! "$confirm" =~ ^[Yy]$ ]]; then
        error "Aborted by user."
        exit 1
    fi
    
    # Add PPA if not already there
    if ! find /etc/apt/sources.list.d -name "*deadsnakes*" | grep -q . 2>/dev/null; then
        info "Adding deadsnakes PPA..."
        sudo add-apt-repository -y ppa:deadsnakes/ppa >/dev/null 2>&1
        sudo apt-get update >/dev/null 2>&1
    else
        info "Deadsnakes PPA already configured"
    fi
    
    # Install Python 3.11
    sudo apt-get install -y python3.11 python3.11-venv python3.11-dev >/dev/null 2>&1
    
    # Install pip if missing
    if ! python3.11 -m pip --version >/dev/null 2>&1; then
        info "Installing pip for Python 3.11..."
        curl -fsSL https://bootstrap.pypa.io/get-pip.py | sudo python3.11 >/dev/null 2>&1
    fi
    
    success "Python 3.11 installed. Use: python3.11"
else
    success "Python $python_version is compatible"
fi

echo -e "\n${GREEN}🎉 Setup complete!${NC}"
echo -e "${CYAN}Next steps:${NC}"
echo -e "  • Run ${YELLOW}python3 --version${NC} (or ${YELLOW}python3.11 --version${NC}) to verify"
echo -e "  • Install additional tools as needed"
echo -e "  • Happy coding!"