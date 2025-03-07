#!/usr/bin/env bash
################################################################################
# Custom Notification Script
#
# This script processes monitor matches and handles custom notification logic.
# It demonstrates basic JSON processing and argument handling.
#
# Input: JSON object containing:
#   - monitor_match: The monitor match data
#   - args: Additional arguments passed to the script
#
# Arguments:
#   --verbose: Enables detailed logging of the processing
#
# Output:
#   - Prints 'true' if processing is successful
#   - Prints 'false' if there are any errors
#   - Includes additional logging when verbose mode is enabled
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
    if [ -z "$input_json" ]; then
        echo "No input JSON provided"
        echo "false"
        exit 1
    fi

    # Validate JSON structure
    if ! echo "$input_json" | jq . >/dev/null 2>&1; then
        echo "Invalid JSON input"
        echo "false"
        exit 1
    fi

    if [ "$verbose" = true ]; then
        echo "Input JSON received:"
        echo "$input_json" | jq '.'
        echo "Monitor match data:"
        echo "$monitor_data" | jq '.'
    fi

    # Process args if they exist
    args_data=$(echo "$input_json" | jq -r '.args')
    if [ "$args_data" != "null" ]; then
        echo "Args: $args_data"
    fi

    # If we made it here, everything worked
    echo "Verbose mode: $verbose"
    echo "true"
    exit 0
}

# Call main function
main
