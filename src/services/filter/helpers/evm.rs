use ethabi::Token;
use web3::types::{H160, H256};

pub fn h256_to_string(hash: H256) -> String {
    format!("0x{}", hex::encode(hash.as_bytes()))
}

pub fn string_to_h256(hash_string: &str) -> Result<H256, Box<dyn std::error::Error>> {
    let hash_without_prefix = hash_string.strip_prefix("0x").unwrap_or(hash_string);
    let hash_bytes = hex::decode(hash_without_prefix)?;
    Ok(H256::from_slice(&hash_bytes))
}

pub fn h160_to_string(address: H160) -> String {
    format!("0x{}", hex::encode(address.as_bytes()))
}

pub fn string_to_h160(address_string: &str) -> Result<H160, Box<dyn std::error::Error>> {
    let address_without_prefix = address_string.strip_prefix("0x").unwrap_or(address_string);
    let address_bytes = hex::decode(address_without_prefix)?;
    Ok(H160::from_slice(&address_bytes))
}

pub fn are_same_address(address1: &str, address2: &str) -> bool {
    normalize_address(address1) == normalize_address(address2)
}

pub fn normalize_address(address: &str) -> String {
    address
        .strip_prefix("0x")
        .unwrap_or(address)
        .replace(" ", "")
        .to_lowercase()
}

pub fn are_same_signature(signature1: &str, signature2: &str) -> bool {
    normalize_signature(signature1) == normalize_signature(signature2)
}

pub fn normalize_signature(signature: &str) -> String {
    signature.replace(" ", "").to_lowercase()
}

pub fn format_token_value(token: &Token) -> String {
    match token {
        Token::Address(addr) => format!("0x{:x}", addr),
        Token::FixedBytes(bytes) | Token::Bytes(bytes) => format!("0x{}", hex::encode(bytes)),
        Token::Int(num) | Token::Uint(num) => num.to_string(), // Decimal representation
        Token::Bool(b) => b.to_string(),
        Token::String(s) => s.clone(),
        Token::Array(arr) => format!(
            "[{}]",
            arr.iter()
                .map(format_token_value)
                .collect::<Vec<String>>()
                .join(",")
        ),
        Token::FixedArray(arr) => format!(
            "[{}]",
            arr.iter()
                .map(format_token_value)
                .collect::<Vec<String>>()
                .join(",")
        ),
        Token::Tuple(tuple) => format!(
            "({})",
            tuple
                .iter()
                .map(format_token_value)
                .collect::<Vec<String>>()
                .join(",")
        ),
    }
}
