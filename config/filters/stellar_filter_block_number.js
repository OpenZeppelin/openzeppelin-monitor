try {
    // Read from stdin instead of command line arguments
    let inputJson = '';
    process.stdin.on('data', chunk => {
        inputJson += chunk;
    });
    
    process.stdin.on('end', () => {
        const data = JSON.parse(inputJson);
        
        // Extract ledger sequence number
        let ledgerNumber = null;
        if (data.Stellar) {
            ledgerNumber = data.Stellar.ledger.sequence;
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