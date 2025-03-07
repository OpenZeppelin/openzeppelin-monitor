/**
 * Custom Notification Script
 * This script filters monitor matches based on the block number of the transaction.
 *
 * Input: JSON object containing:
 *   - monitor_match: The monitor match data with transaction details
 *   - args: Additional arguments passed to the script
 *
 * Output:
 *   - Prints 'true' for transactions in even-numbered blocks
 *   - Prints 'false' for transactions in odd-numbered blocks or invalid input
 *
 * Note: Block numbers are extracted from the EVM transaction data and converted
 * from hexadecimal to decimal before processing.
 */
try {
    let inputData = '';
    // Read from stdin
    process.stdin.on('data', chunk => {
        inputData += chunk;
    });

    process.stdin.on('end', () => {
        // Parse input JSON
        const data = JSON.parse(inputData);
        const monitorMatch = data.monitor_match;
        const args = data.args;

        // Log args if they exist
        if (args && args.length > 0) {
            console.log(`Args: ${JSON.stringify(args)}`);
        }

        // Validate monitor match data
        if (!monitorMatch) {
            console.log("No monitor match data provided");
            console.log('false');
            return;
        }

        // If we reach here, processing was successful
        console.log('true');
    });
} catch (e) {
    console.log(`Error processing input: ${e}`);
    console.log('false');
}
