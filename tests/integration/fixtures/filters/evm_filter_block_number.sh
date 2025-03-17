#!/usr/bin/env bash

# Enable error handling
set -e

main() {
    verbose=false

    # Read JSON input from stdin
    input_json=$(cat)

    # Parse arguments from the input JSON
    args=$(echo "$input_json" | jq -r '.args // empty')
    if [ ! -z "$args" ]; then
        if [[ $args == *"--verbose"* ]]; then
            verbose=true
            echo "Verbose mode enabled"
        fi
    fi

    # Extract the monitor match data from the input
    monitor_data=$(echo "$input_json" | jq -r '.monitor_match')

    # Validate input
    if [ -z "$monitor_data" ]; then
        echo "No input JSON provided"
        echo "false"
        exit 1
    fi

    if [ "$verbose" = true ]; then
        echo "Input JSON received:"
    fi

    # Extract blockNumber from the EVM receipt or transaction
    block_number_hex=$(echo "$monitor_data" | jq -r '.EVM.transaction.blockNumber' || echo "")

    # Validate that block_number_hex is not empty
    if [ -z "$block_number_hex" ]; then
        echo "Invalid JSON or missing blockNumber"
        echo "false"
        exit 1
    fi

    # Remove 0x prefix if present and clean the string
    block_number_hex=$(echo "$block_number_hex" | tr -d '\n' | tr -d ' ')
    block_number_hex=${block_number_hex#0x}

    if [ "$verbose" = true ]; then
        echo "Extracted block number (hex): $block_number_hex"
    fi

    # Convert hex to decimal with error checking
    if ! block_number=$(printf "%d" $((16#${block_number_hex})) 2>/dev/null); then
        echo "Failed to convert hex to decimal"
        echo "false"
        exit 1
    fi

    if [ "$verbose" = true ]; then
        echo "Converted block number (decimal): $block_number"
    fi

    # Check if even or odd using modulo
    is_even=$((block_number % 2))

    if [ $is_even -eq 0 ]; then
        echo "Block number $block_number is even"
        echo "Verbose mode: $verbose"
        echo "true"
        exit 0
    else
        echo "Block number $block_number is odd"
        echo "Verbose mode: $verbose"
        echo "false"
        exit 0
    fi
}

# Call main function
main
