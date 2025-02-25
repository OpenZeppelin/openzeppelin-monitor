#!/usr/bin/env python3
import sys
import json
import logging

def main():
    try:
        # Read input from stdin instead of command line arguments
        input_data = sys.stdin.read()
        
        # Parse input JSON
        try:
            data = json.loads(input_data)
        except json.JSONDecodeError:
            return False

        # Extract ledger_number
        ledger_number = None
        if "Stellar" in data:
            ledger = data['Stellar']['ledger'].get('sequence')
            if ledger:
                ledger_number = int(ledger)

        if ledger_number is None:
            return False

        # Return True for even ledger numbers, False for odd
        result = ledger_number % 2 == 0
        print(f"Ledger number {ledger_number} is {'even' if result else 'odd'}", flush=True)
        return result

    except Exception:
        return False

if __name__ == "__main__":
    result = main()
    # Only print the final boolean result
    print(str(result).lower(), flush=True)