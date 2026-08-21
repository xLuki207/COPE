use anyhow::Result;
use bs58::decode;
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SolanaAddress(pub String);

impl SolanaAddress {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExtractResult {
    None,
    Single(SolanaAddress),
    Multiple(Vec<SolanaAddress>),
}

pub fn extract_solana_addresses(text: &str) -> Result<ExtractResult> {
    let candidates = find_base58_candidates(text);
    let mut valid_addresses = HashSet::new();

    for candidate in candidates {
        if let Ok(decoded) = decode(&candidate).into_vec() {
            if decoded.len() == 32 {
                valid_addresses.insert(SolanaAddress(candidate));
            }
        }
    }

    let addresses: Vec<SolanaAddress> = valid_addresses.into_iter().collect();

    Ok(match addresses.len() {
        0 => ExtractResult::None,
        1 => ExtractResult::Single(addresses[0].clone()),
        _ => ExtractResult::Multiple(addresses),
    })
}

fn find_base58_candidates(text: &str) -> Vec<String> {
    const BASE58_ALPHABET: &str = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    let mut candidates = Vec::new();
    let mut current = String::new();

    for ch in text.chars() {
        if BASE58_ALPHABET.contains(ch) {
            current.push(ch);
        } else {
            if current.len() >= 32 && current.len() <= 44 {
                candidates.push(current.clone());
            }
            current.clear();
        }
    }

    if current.len() >= 32 && current.len() <= 44 {
        candidates.push(current);
    }

    candidates
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_CA_1: &str = "7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU";
    const VALID_CA_2: &str = "9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM";

    #[test]
    fn test_plain_valid_ca() {
        let result = extract_solana_addresses(VALID_CA_1).unwrap();
        assert_eq!(
            result,
            ExtractResult::Single(SolanaAddress(VALID_CA_1.to_string()))
        );
    }

    #[test]
    fn test_ca_inside_prose() {
        let text = format!("bro look {} this might run", VALID_CA_1);
        let result = extract_solana_addresses(&text).unwrap();
        assert_eq!(
            result,
            ExtractResult::Single(SolanaAddress(VALID_CA_1.to_string()))
        );
    }

    #[test]
    fn test_multiline_text() {
        let text = format!("yo look at this\n\nCA: {}\n\nmight send", VALID_CA_1);
        let result = extract_solana_addresses(&text).unwrap();
        assert_eq!(
            result,
            ExtractResult::Single(SolanaAddress(VALID_CA_1.to_string()))
        );
    }

    #[test]
    fn test_invalid_base58() {
        let text = "not a valid address !@#$%^&*()";
        let result = extract_solana_addresses(text).unwrap();
        assert_eq!(result, ExtractResult::None);
    }

    #[test]
    fn test_wrong_byte_length() {
        let short = "7xKXtg2CW87d97TXJSDpbD5jBkheTqA8";
        let result = extract_solana_addresses(short).unwrap();
        assert_eq!(result, ExtractResult::None);

        let long = "7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsUExtra";
        let result = extract_solana_addresses(long).unwrap();
        assert_eq!(result, ExtractResult::None);
    }

    #[test]
    fn test_duplicate_same_ca() {
        let text = format!("{} and again {}", VALID_CA_1, VALID_CA_1);
        let result = extract_solana_addresses(&text).unwrap();
        assert_eq!(
            result,
            ExtractResult::Single(SolanaAddress(VALID_CA_1.to_string()))
        );
    }

    #[test]
    fn test_two_different_valid_cas() {
        let text = format!("{} {}", VALID_CA_1, VALID_CA_2);
        let result = extract_solana_addresses(&text).unwrap();
        match result {
            ExtractResult::Multiple(addrs) => {
                assert_eq!(addrs.len(), 2);
                assert!(addrs.contains(&SolanaAddress(VALID_CA_1.to_string())));
                assert!(addrs.contains(&SolanaAddress(VALID_CA_2.to_string())));
            }
            _ => panic!("Expected Multiple"),
        }
    }

    #[test]
    fn test_no_ca() {
        let result = extract_solana_addresses("hello world").unwrap();
        assert_eq!(result, ExtractResult::None);
    }

    #[test]
    fn test_punctuation_around_ca() {
        let text = format!("CA: {}", VALID_CA_1);
        let result = extract_solana_addresses(&text).unwrap();
        assert_eq!(
            result,
            ExtractResult::Single(SolanaAddress(VALID_CA_1.to_string()))
        );

        let text = format!("({})", VALID_CA_1);
        let result = extract_solana_addresses(&text).unwrap();
        assert_eq!(
            result,
            ExtractResult::Single(SolanaAddress(VALID_CA_1.to_string()))
        );

        let text = format!("[{}]", VALID_CA_1);
        let result = extract_solana_addresses(&text).unwrap();
        assert_eq!(
            result,
            ExtractResult::Single(SolanaAddress(VALID_CA_1.to_string()))
        );
    }

    #[test]
    fn test_url_containing_address() {
        let text = format!("https://solscan.io/token/{}", VALID_CA_1);
        let result = extract_solana_addresses(&text).unwrap();
        assert_eq!(
            result,
            ExtractResult::Single(SolanaAddress(VALID_CA_1.to_string()))
        );
    }
}
