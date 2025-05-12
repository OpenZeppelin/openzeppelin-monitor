#!/usr/bin/env python3
"""
Custom Notification Script
This script validates monitor match data and logs validation results to stderr.

Input: JSON object containing:
    - monitor_match: The monitor match data with transaction details
    - args: Additional arguments passed to the script (optional)

Note: Only stderr output is monitored. If the script returns a non-zero exit code, the error will be logged.
"""
import sys
import json

def main():
    try:
        # Read input from stdin
        input_data = sys.stdin.read()
        if not input_data:
            print("No input JSON provided", flush=True)

        # Parse input JSON
        try:
            data = json.loads(input_data)
            monitor_match = data['monitor_match']
            args = data['args']
            if args:
                print(f"Args: {args}")
        except json.JSONDecodeError as e:
            print(f"Invalid JSON input: {e}", flush=True)


    except Exception as e:
        print(f"Error processing input: {e}", flush=True)

if __name__ == "__main__":
    main()
