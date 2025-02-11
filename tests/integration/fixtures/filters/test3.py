"""
The processed_block dictionary has the following shape:

{
    "block_number": u64,  # Block number of the transaction
    "network_slug": str,  # Network identifier (e.g. "ethereum-mainnet", "polygon-mainnet")
    "processing_results": [# Array of MonitorMatch objects containing match results]
}

The filter function below determines whether to trigger alerts based on the block number.
"""
def filter_block_number(processed_block: dict) -> bool:
    """
    Filter function that returns True if the block number is even.
    
    Args:
        processed_block (dict): A dictionary containing either EVM or Stellar transaction data
                            with 'block_number' field indicating the block number
        
    Returns:
        bool: True if the block number is even, False otherwise
    """
    block_number = processed_block.get('block_number', 0)
    return block_number % 2 == 0