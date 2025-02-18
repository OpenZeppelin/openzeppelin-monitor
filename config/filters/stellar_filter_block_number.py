#!/usr/bin/env python3
import sys
import json
import logging

def main():
    try:
        # Validate we have the input argument
        if len(sys.argv) < 2:
            print("No input JSON provided", flush=True)
            return False

        # Parse input JSON
        try:
            data = json.loads(sys.argv[1])
        except json.JSONDecodeError as e:
            print(f"Invalid JSON input: {e}", flush=True)
            return False

        # Extract ledger_number
        ledger_number = None
        if "Stellar" in data:
            ledger = data['Stellar']['transaction'].get('ledger')
            if ledger:
                ledger_number = int(ledger, 16)

        if ledger_number is None:
            print("Ledger number is None")
            return False

        result = ledger_number % 2 == 0
        print(f"Ledger number {ledger_number} is {'even' if result else 'odd'}", flush=True)
        logging.info(f"Ledger number {ledger_number} is {'even' if result else 'odd'}")
        return result

    except Exception as e:
        print(f"Error processing input: {e}", flush=True)
        return False

if __name__ == "__main__":
    result = main()
    # Print the final boolean result
    print(str(result).lower(), flush=True)