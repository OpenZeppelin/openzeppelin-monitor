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
        except json.JSONDecodeError:
            print("Invalid JSON input", flush=True)
            return False

        # Extract ledger_number
        ledger_number = None
        if "Stellar" in monitor_match:
            ledger = monitor_match['Stellar']['ledger'].get('sequence')
            if ledger:
                ledger_number = int(ledger)

        if ledger_number is None:
            print("Ledger number is None", flush=True)
            return False

        # Return True for even ledger numbers, False for odd
        result = ledger_number % 2 == 0
        print(f"Ledger number {ledger_number} is {'even' if result else 'odd'}", flush=True)
        return result

    except Exception as e:
        print(f"Error processing input: {e}", flush=True)
        return False

if __name__ == "__main__":
    result = main()
    # Only print the final boolean result
    print(str(result).lower(), flush=True)
