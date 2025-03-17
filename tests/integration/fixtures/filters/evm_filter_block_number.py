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
            monitor_match = data['monitor_match']
            args = data['args']
        except json.JSONDecodeError as e:
            print(f"Invalid JSON input: {e}", flush=True)
            return False

        # Extract block_number
        block_number = None
        if "EVM" in monitor_match:
            hex_block = monitor_match['EVM']['transaction'].get('blockNumber')
            if hex_block:
                # Convert hex string to integer
                block_number = int(hex_block, 16)

        if block_number is None:
            print("Block number is None")
            return False

        result = block_number % 2 == 0
        print(f"Block number {block_number} is {'even' if result else 'odd'}", flush=True)
        logging.info(f"Block number {block_number} is {'even' if result else 'odd'}")
        return result

    except Exception as e:
        print(f"Error processing input: {e}", flush=True)
        return False

if __name__ == "__main__":
    result = main()
    # Print the final boolean result
    print(str(result).lower(), flush=True)
