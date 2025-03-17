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
            console.log("Ledger number is None");
            console.log('false');
            return;
        }

        const result = ledgerNumber % 2 === 0;
        console.log(`Ledger number ${ledgerNumber} is ${result ? 'even' : 'odd'}`);
        console.log(result.toString());
    });

} catch (e) {
    console.log(`Error processing input: ${e}`);
    console.log('false');
}
