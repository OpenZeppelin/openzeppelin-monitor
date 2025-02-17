#!/usr/bin/env node

const main = () => {
    try {
        // Validate we have the input argument
        if (process.argv.length < 3) {
            console.log("No input JSON provided");
            return false;
        }

        // Parse input JSON
        let data;
        try {
            data = JSON.parse(process.argv[2]);
        } catch (e) {
            console.log(`Invalid JSON input: ${e}`);
            return false;
        }

        // Extract block_number
        let blockNumber = null;
        if (data.EVM) {
            const hexBlock = data.EVM.transaction?.blockNumber;
            if (hexBlock) {
                // Convert hex string to integer
                blockNumber = parseInt(hexBlock, 16);
                console.log(`BLOCK NUMBER INTEGER ==>: ${blockNumber}`);
            }
        }

        if (blockNumber === null) {
            console.log("Block number is None");
            return false;
        }

        const result = blockNumber % 2 === 0;
        console.log(`Block number ${blockNumber} is ${result ? 'even' : 'odd'}`);
        // Note: For logging, you might want to use a proper logging library like winston
        // but for this example, we'll use console.log
        console.log(`Block number ${blockNumber} is ${result ? 'even' : 'odd'}`);
        return result;

    } catch (e) {
        console.log(`Error processing input: ${e}`);
        return false;
    }
};

if (require.main === module) {
    const result = main();
    // Print the final boolean result
    console.log(result.toString());
}