try {
    const inputJson = process.argv[1];
    const data = JSON.parse(inputJson);

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
        console.log('false');
    }

    const result = blockNumber % 2 === 0;
    console.log(`Block number ${blockNumber} is ${result ? 'even' : 'odd'}`);
    console.log(result.toString());

} catch (e) {
    console.log(`Error processing input: ${e}`);
    console.log('false');
}