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

    # Validate input
    if [ -z "$input_json" ]; then
        echo "No input JSON provided"
        echo "false"
        exit 1
    fi

    if [ "$verbose" = true ]; then
        echo "Input JSON received:"
    fi

    # Extract ledger number from the nested monitor_match.Stellar structure
    ledger_number=$(echo "$input_json" | jq -r '.monitor_match.Stellar.ledger.sequence // empty')

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

# Call main function without arguments, input will be read from stdin
main