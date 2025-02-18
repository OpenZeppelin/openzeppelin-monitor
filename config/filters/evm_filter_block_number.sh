#!/bin/bash

main() {
    # Validate we have the input argument
    if [ $# -lt 1 ]; then
        echo "No input JSON provided"
        echo "false"
        exit 1
    fi

    # Parse input JSON and extract block number
    # Using jq to safely parse JSON and extract the blockNumber
    block_number_hex=$(echo "$1" | jq -r '.EVM.transaction.blockNumber // empty')

    # Convert hex to decimal
    # Remove '0x' prefix if present and convert using printf
    block_number=$(printf "%d" $((16#${block_number_hex#0x})))

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

# Catch any errors
set -e

# Call main function with all arguments
main "$@"