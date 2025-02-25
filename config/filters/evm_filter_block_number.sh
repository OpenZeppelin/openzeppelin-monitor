#!/usr/bin/env bash

# Enable error handling and debug output
set -e
set -x  # Add debug output to see what's happening

main() {
    # Read JSON input from stdin
    input_json=$(cat)

    # Validate input
    if [ -z "$input_json" ]; then
        echo "No input JSON provided"
        echo "false"
        exit 1
    fi

    block_number_hex=$(echo "$input_json" | grep -o '"blockNumber":"[^"]*"' | head -n1 | cut -d'"' -f4 || echo "")

    # Validate that block_number_hex is not empty
    if [ -z "$block_number_hex" ]; then
        echo "Invalid JSON or missing blockNumber"
        echo "false"
        exit 1
    fi

    # Remove 0x prefix if present and clean the string
    block_number_hex=$(echo "$block_number_hex" | tr -d '\n' | tr -d ' ')
    block_number_hex=${block_number_hex#0x}

    # Convert hex to decimal with error checking
    if ! block_number=$(printf "%d" $((16#${block_number_hex})) 2>/dev/null); then
        echo "Failed to convert hex to decimal"
        echo "false"
        exit 1
    fi

    # Check if even or odd using modulo
    is_even=$((block_number % 2))
    
    if [ $is_even -eq 0 ]; then
        echo "Block number $block_number is even"
        echo "true"
        exit 0
    else
        echo "Block number $block_number is odd"
        echo "false"
        exit 0
    fi
}

# Call main function without arguments and let it read from stdin
main
