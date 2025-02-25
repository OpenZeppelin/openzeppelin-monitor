#!/bin/bash

# Enable error handling
set -e

main() {
    # Read JSON input from stdin instead of argument
    input_json=$(cat)

    # Validate input
    if [ -z "$input_json" ]; then
        echo "No input JSON provided"
        echo "false"
        exit 1
    fi

    # Extract ledger number using grep and cut
    ledger_number=$(echo "$input_json" | grep -o '"sequence":[^,}]*' | head -n1 | cut -d':' -f2 || echo "")

    # Validate ledger number
    if [ -z "$ledger_number" ]; then
        echo "Invalid JSON or missing sequence number"
        echo "false"
        exit 1
    fi

    # Remove any whitespace
    ledger_number=$(echo "$ledger_number" | tr -d '\n' | tr -d ' ')

    # Check if even or odd using modulo
    is_even=$((ledger_number % 2))
    
    if [ $is_even -eq 0 ]; then
        echo "Ledger number $ledger_number is even"
        echo "true"
        exit 0
    else
        echo "Ledger number $ledger_number is odd"
        echo "false"
        exit 0
    fi
}

# Call main function without arguments, input will be read from stdin
main