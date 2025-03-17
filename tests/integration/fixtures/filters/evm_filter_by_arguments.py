#!/usr/bin/env python3
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
            args = data['args']
        except json.JSONDecodeError as e:
            print(f"Invalid JSON input: {e}", flush=True)
            return False

        # Check if --verbose is in args
        result = '--verbose' in args
        print(f"Verbose mode is {'enabled' if result else 'disabled'}", flush=True)
        logging.info(f"Verbose mode is {'enabled' if result else 'disabled'}")
        return result

    except Exception as e:
        print(f"Error processing input: {e}", flush=True)
        return False

if __name__ == "__main__":
    result = main()
    # Print the final boolean result
    print(str(result).lower(), flush=True)
