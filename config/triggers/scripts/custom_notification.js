/**
 * Custom Notification Script
 * This script validates monitor match data and logs validation results to stderr.
 *
 * Input: JSON object containing:
 *   - monitor_match: The monitor match data with transaction details
 *   - args: Additional arguments passed to the script (optional)
 *
 * Note: Only stderr output is monitored. If the script returns a non-zero exit code, the error will be logged.
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
            return;
        }
    });
} catch (e) {
    console.log(`Error processing input: ${e}`);
}
