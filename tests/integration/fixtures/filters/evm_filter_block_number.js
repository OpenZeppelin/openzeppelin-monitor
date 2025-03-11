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
                console.log(`BLOCK NUMBER INTEGER ==>: ${blockNumber}`);
            }
        }

        if (blockNumber === null) {
            console.log("Block number is None");
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
