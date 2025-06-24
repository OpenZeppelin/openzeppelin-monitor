#!/bin/bash

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration to copy
NETWORKS=(
    "ethereum_mainnet.json"
    "stellar_mainnet.json"
)

MONITORS=(
    "evm_transfer_usdc.json"
    "stellar_swap_dex.json"
)

FILTERS=(
    "evm_filter_block_number.sh"
    "stellar_filter_block_number.sh"
)

TRIGGERS=(
    "discord_notifications.json"
    "email_notifications.json"
    "script_notifications.json"
    "slack_notifications.json"
    "webhook_notifications.json"
)

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

    for network_file in "${NETWORKS[@]}"; do
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
    monitor_count=0

    for monitor_file in "${MONITORS[@]}"; do
        if [ -f "examples/config/monitors/$monitor_file" ]; then
            # Use jq if available to set triggers to empty array, otherwise just copy
            if command_exists jq; then
                jq '.triggers = []' "examples/config/monitors/$monitor_file" > "config/monitors/$monitor_file"
                print_success "Copied and modified $monitor_file (triggers set to empty array for initial setup)"
            else
                cp "examples/config/monitors/$monitor_file" "config/monitors/"
                print_warning "Copied $monitor_file (jq not available - triggers not modified automatically)"
            fi
            monitor_count=$((monitor_count + 1))
        else
            print_warning "$monitor_file not found in examples/config/monitors/"
        fi
    done

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
    filter_count=0

    for filter_file in "${FILTERS[@]}"; do
        if [ -f "examples/config/filters/$filter_file" ]; then
            cp "examples/config/filters/$filter_file" "config/filters/"
            chmod +x "config/filters/$filter_file"
            print_success "Copied $filter_file and made it executable"
            filter_count=$((filter_count + 1))
        else
            print_warning "$filter_file not found in examples/config/filters/"
        fi
    done

    if [ "$filter_count" -gt 0 ]; then
        print_success "Copied $filter_count filter script(s)"
    else
        print_warning "No filter scripts found to copy"
    fi
else
    print_warning "examples/config/filters directory not found"
fi

# Copy trigger configurations
print_status "Copying trigger configurations..."
if [ -d "examples/config/triggers" ]; then
    trigger_count=0

    for trigger_file in "${TRIGGERS[@]}"; do
        if [ -f "examples/config/triggers/$trigger_file" ]; then
            cp "examples/config/triggers/$trigger_file" "config/triggers/"
            print_success "Copied $trigger_file"
            trigger_count=$((trigger_count + 1))
        else
            print_warning "$trigger_file not found in examples/config/triggers/"
        fi
    done

    if [ "$trigger_count" -gt 0 ]; then
        print_success "Copied $trigger_count trigger configuration(s)"
    else
        print_warning "No trigger configurations found to copy"
    fi
else
    print_warning "examples/config/triggers directory not found"
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
    echo "2. Customize trigger configurations in config/triggers/"
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
