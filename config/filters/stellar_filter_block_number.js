/**
 * Stellar Block Number Filter
 *
 * This script filters monitor matches based on the block number of the transaction.
 * It demonstrates a simple filter that only allows transactions from even-numbered blocks.
 *
 * Input: JSON object containing:
 *   - monitor_match: The monitor match data with transaction details
 *   - args: Additional arguments passed to the script
 *
 * Output:
 *   - Prints 'true' for transactions in even-numbered blocks
 *   - Prints 'false' for transactions in odd-numbered blocks or invalid input
 */
try {
    // Read from stdin
    let inputData = '';
    process.stdin.on('data', chunk => {
        inputData += chunk;
    });

    process.stdin.on('end', () => {
        const data = JSON.parse(inputData);
        const monitorMatch = data.monitor_match;
        const args = data.args;

        // Extract ledger sequence number
        let ledgerNumber = null;
        if (monitorMatch.Stellar) {
            ledgerNumber = monitorMatch.Stellar.ledger.sequence;
        }

        if (ledgerNumber === null) {
            console.log('false');
            return;
        }

        const result = ledgerNumber % 2 === 0;
        console.log(result.toString());
    });

} catch (e) {
    console.log(`Error processing input: ${e}`);
    console.log('false');
}
