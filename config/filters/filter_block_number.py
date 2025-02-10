"""
 ProcessedBlock is a dictionary containing either EVM or Stellar transaction data with the following fields:
    block_number: u64,
	network_slug: String,
	processing_results: Array[MonitorMatch],
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