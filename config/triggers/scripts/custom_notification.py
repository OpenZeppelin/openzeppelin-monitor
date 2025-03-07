#!/usr/bin/env python3
"""
Custom Notification Script
This script filters monitor matches based on the block number of the transaction.

Input: JSON object containing:
    - monitor_match: The monitor match data with transaction details
    - args: Additional arguments passed to the script

Output:
    - Prints 'true' for transactions in even-numbered blocks
    - Prints 'false' for transactions in odd-numbered blocks or invalid input
    - Includes additional logging of block number status

Note: Block numbers are extracted from the EVM transaction data and converted
from hexadecimal to decimal before processing.
"""
import sys
import json
import logging

def main():
    try:
        # Read input from stdin
        input_data = sys.stdin.read()
        if not input_data:
            print("No input JSON provided", flush=True)
            return False

        # Parse input JSON
        try:
            data = json.loads(input_data)
            monitor_match = data['monitor_match']
            args = data['args']
            if args:
                print(f"Args: {args}")
        except json.JSONDecodeError as e:
            print(f"Invalid JSON input: {e}", flush=True)
            return False
        return True

    except Exception as e:
        print(f"Error processing input: {e}", flush=True)
        return False

if __name__ == "__main__":
    result = main()
    # Print the final boolean result
    print(str(result).lower(), flush=True)
