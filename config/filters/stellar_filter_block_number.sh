#!/bin/bash
################################################################################
# Stellar Block Number Filter
#
# This script filters monitor matches based on the block number of the transaction.
# It demonstrates a simple filter that only allows transactions from even-numbered blocks.
#
# Input: JSON object containing:
#   - monitor_match: The monitor match data with transaction details
#   - args: Additional arguments passed to the script
#
# Arguments:
#   --verbose: Enables detailed logging of the filtering process
#
# Output:
#   - Prints 'true' for transactions in even-numbered blocks
#   - Prints 'false' for transactions in odd-numbered blocks or invalid input
#   - Includes additional logging when verbose mode is enabled
#
################################################################################

# Enable error handling
set -e

main() {
    # Read JSON input from stdin
    input_json=$(cat)

    # Parse arguments from the input JSON and initialize verbose flag
    verbose=false
    args=$(echo "$input_json" | jq -r '.args[]? // empty')
    if [ ! -z "$args" ]; then
        while IFS= read -r arg; do
            if [ "$arg" = "--verbose" ]; then
                verbose=true
                echo "Verbose mode enabled"
            fi
        done <<< "$args"
    fi

    # Extract the monitor match data from the input
    monitor_data=$(echo "$input_json" | jq -r '.monitor_match')

    # Validate input
    if [ -z "$monitor_data" ]; then
        echo "No input JSON provided"
        echo "false"
        exit 1
    fi

    # Extract ledger Number
    ledger_number=$(echo "$monitor_data" | jq -r '.Stellar.ledger.sequence' || echo "")

    # Validate ledger number
    if [ -z "$ledger_number" ] || [ "$ledger_number" = "null" ]; then
        echo "Invalid JSON or missing sequence number"
        echo "false"
        exit 1
    fi

    if [ "$verbose" = true ]; then
        echo "Ledger number: $ledger_number"
    fi

    # Check if even or odd using modulo
    is_even=$((ledger_number % 2))

    if [ $is_even -eq 0 ]; then
        echo "Ledger number $ledger_number is even"
        echo "Verbose mode: $verbose"
        echo "true"
        exit 0
    else
        echo "Ledger number $ledger_number is odd"
        echo "Verbose mode: $verbose"
        echo "false"
        exit 0
    fi
}

# Call main function
main
