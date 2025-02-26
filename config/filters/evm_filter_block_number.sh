#!/usr/bin/env bash

# Enable error handling
set -e

main() {
    verbose=false
    # Process command line arguments first
    while [[ $# -gt 0 ]]; do
        case $1 in
            --verbose)
                verbose=true
                shift # move to next argument
                ;;
            *)
                shift # ignore unknown arguments
                ;;
        esac
    done

    if [ "$verbose" = true ]; then
        echo "Verbose mode enabled"
    fi

    # Read JSON input from stdin
    input_json=$(cat)

    # Validate input
    if [ -z "$input_json" ]; then
        echo "No input JSON provided"
        echo "false"
        exit 1
    fi

    if [ "$verbose" = true ]; then
        echo "Verbose mode enabled"
        echo "Input JSON received:"
        echo "$input_json"
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

    if [[ "$*" == *"--verbose"* ]]; then
        echo "Extracted block number (hex): $block_number_hex"
    fi

    # Convert hex to decimal with error checking
    if ! block_number=$(printf "%d" $((16#${block_number_hex})) 2>/dev/null); then
        echo "Failed to convert hex to decimal"
        echo "false"
        exit 1
    fi

    if [[ "$*" == *"--verbose"* ]]; then
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

# Call main function with all arguments
main "$@"
