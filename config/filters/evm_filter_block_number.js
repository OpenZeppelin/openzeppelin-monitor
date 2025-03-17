/**
 * EVM Block Number Filter
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
        const data = JSON.parse(inputData);
        const monitorMatch = data.monitor_match;
        const args = data.args;

        // Extract block_number
        let blockNumber = null;
        if (monitorMatch.EVM) {
            const hexBlock = monitorMatch.EVM.transaction?.blockNumber;
            if (hexBlock) {
                // Convert hex string to integer
                blockNumber = parseInt(hexBlock, 16);
            }
        }

        if (blockNumber === null) {
            console.log('false');
            return;
        }

        const result = blockNumber % 2 === 0;
        console.log(`Block number ${blockNumber} is ${result ? 'even' : 'odd'}`);
        console.log(result.toString());
    });
} catch (e) {
    console.log(`Error processing input: ${e}`);
    console.log('false');
}
