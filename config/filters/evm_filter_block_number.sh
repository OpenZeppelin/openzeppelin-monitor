#!/bin/bash

main() {
    # Read JSON input from stdin
    input_json=$(cat)

    # Validate input
    if [[ -z "$input_json" ]]; then
        echo "No input JSON provided"
        echo "false"
        exit 1
    fi

    # Parse input JSON and extract block number
    block_number_hex=$(echo "$input_json" | jq -r '.EVM.transaction.blockNumber // empty')

    # Validate that block_number_hex is not empty
    if [[ -z "$block_number_hex" ]]; then
        echo "Invalid JSON or missing blockNumber"
        echo "false"
        exit 1
    fi

    # Convert hex to decimal
    block_number=$(printf "%d" $((16#${block_number_hex#0x})))

    # Check if even or odd using modulo
    is_even=$((block_number % 2))
    
    if [[ $is_even -eq 0 ]]; then
        echo "Block number $block_number is even"
        echo "true"
        exit 0
    else
        echo "Block number $block_number is odd"
        echo "false"
        exit 0
    fi
}

# Enable error handling
set -e

# Call main function
main
