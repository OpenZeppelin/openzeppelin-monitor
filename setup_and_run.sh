#!/bin/bash

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Function to print colored output
print_status() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Function to check if command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

# Check if we're in the right directory
if [ ! -f "Cargo.toml" ]; then
    print_error "Cargo.toml file not found. Please run this script from the openzeppelin-monitor root directory."
    exit 1
fi

print_status "🚀 Setting up OpenZeppelin Monitor configurations..."

# Check if Rust is available
if ! command_exists rustc; then
    print_error "Rust not found. Please install Rust first: https://rustup.rs/"
    exit 1
fi

print_success "Rust is available ($(rustc --version))"

# Build a release binary
print_status "Building monitor binary from source..."
if cargo build --release; then
    mv ./target/release/openzeppelin-monitor .
    chmod +x ./openzeppelin-monitor
    print_success "Monitor binary built successfully!"
else
    print_error "Failed to build monitor binary. Please check the error messages above."
    exit 1
fi

# Create config directories
print_status "Creating configuration directories..."
mkdir -p config/{networks,monitors,triggers,filters}

# Copy network configurations
print_status "Copying network configurations..."
if [ -d "examples/config/networks" ]; then
    network_count=0

    # Copy specific network files
    for network_file in "ethereum_mainnet.json" "stellar_mainnet.json"; do
        if [ -f "examples/config/networks/$network_file" ]; then
            cp "examples/config/networks/$network_file" "config/networks/"
            print_success "Copied $network_file"
            network_count=$((network_count + 1))
        else
            print_warning "$network_file not found in examples/config/networks/"
        fi
    done

    if [ "$network_count" -gt 0 ]; then
        print_success "Copied $network_count network configuration(s)"
    else
        print_warning "No target network configurations found to copy"
    fi
else
    print_warning "examples/config/networks directory not found"
fi

# Copy monitor configurations
print_status "Copying monitor configurations..."
if [ -d "examples/config/monitors" ]; then
    # Copy monitors but modify them to set triggers to empty array
    for monitor_file in examples/config/monitors/*.json; do
        if [ -f "$monitor_file" ]; then
            filename=$(basename "$monitor_file")
            # Use jq if available to set triggers to empty array, otherwise just copy
            if command_exists jq; then
                jq '.triggers = []' "$monitor_file" > "config/monitors/$filename"
                print_success "Copied and modified $filename (triggers set to empty array for initial setup)"
            else
                cp "$monitor_file" "config/monitors/"
                print_warning "Copied $filename (jq not available - triggers not modified automatically)"
            fi
        fi
    done

    monitor_count=$(ls config/monitors/*.json 2>/dev/null | wc -l)
    if [ "$monitor_count" -gt 0 ]; then
        print_success "Copied $monitor_count monitor configuration(s)"
    else
        print_warning "No monitor configurations found to copy"
    fi
else
    print_warning "examples/config/monitors directory not found"
fi

# Copy filter scripts
print_status "Copying filter scripts..."
if [ -d "examples/config/filters" ]; then
    # Only copy .sh files
    if ls examples/config/filters/*.sh 1> /dev/null 2>&1; then
        cp examples/config/filters/*.sh config/filters/
        # Make scripts executable
        chmod +x config/filters/*.sh 2>/dev/null
        filter_count=$(ls config/filters/*.sh 2>/dev/null | wc -l)
        if [ "$filter_count" -gt 0 ]; then
            print_success "Copied $filter_count shell script(s) and made them executable"
        else
            print_warning "No shell scripts found after copying"
        fi
    else
        print_warning "No .sh filter scripts found to copy"
    fi
else
    print_warning "examples/config/filters directory not found"
fi

# Set up environment file if it doesn't exist
if [ ! -f ".env" ]; then
    if [ -f ".env.example" ]; then
        cp .env.example .env
        print_success "Environment file created from .env.example"
    else
        print_warning ".env.example not found. You may need to create a .env file manually."
    fi
else
    print_success "Environment file already exists"
fi

# Validate configurations
print_status "Validating configurations..."
if ./openzeppelin-monitor --check; then
    print_success "✅ Configuration validation passed!"

    echo ""
    print_status "📋 Setup completed successfully! Here's what was configured:"
    echo ""
    echo "📁 Networks: $(ls config/networks/*.json 2>/dev/null | wc -l) configuration(s)"
    echo "📊 Monitors: $(ls config/monitors/*.json 2>/dev/null | wc -l) configuration(s)"
    echo "🔧 Filters: $(ls config/filters/ 2>/dev/null | wc -l) script(s)"
    echo "📢 Triggers: Template created (requires your credentials)"
    echo ""

    print_status "🔧 Next steps to enable notifications:"
    echo "1. Modify monitor configurations to add triggers:"
    echo "   - Edit files in config/monitors/"
    echo "   - Change 'triggers': [] to 'triggers': [\"your_notification_file_name\"] to enable notifications"
    echo ""
    echo "2. Customize trigger configurations in config/triggers/notifications.json"
    echo ""

    # Ask if user wants to run the monitor
    read -p "Would you like to start the monitor now? (y/N): " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        print_status "🚀 Starting OpenZeppelin Monitor..."
        echo ""
        print_warning "Note: Monitors won't send notifications until you add trigger names to the triggers array!"
        echo ""
        exec ./openzeppelin-monitor
    else
        echo ""
        print_status "Setup complete! To start monitoring, run:"
        echo "./openzeppelin-monitor"
        echo ""
        print_status "To test a specific monitor, run:"
        echo "./openzeppelin-monitor --monitor-path=\"config/monitors/[monitor_file].json\""
        echo ""
        print_status "To validate configurations anytime, run:"
        echo "./openzeppelin-monitor --check"
    fi

else
    print_error "❌ Configuration validation failed!"
    print_status "Fix the issues above and run './openzeppelin-monitor --check' to validate again."
    exit 1
fi
