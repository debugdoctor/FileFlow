use std::time::{SystemTime, UNIX_EPOCH};

// Restrict IDs to 5 characters using digits + lowercase letters
const ALPHABET: &str = "0123456789abcdefghijklmnopqrstuvwxyz";
const DEFAULT_SIZE: usize = 5;

/// using the default alphabet
pub fn generate() -> String {
    generate_with_length(DEFAULT_SIZE)
}

/// Generate a random nanoid string with specified length
pub fn generate_with_length(length: usize) -> String {
    generate_custom(length, ALPHABET)
}

/// Generate a random nanoid string with specified length and alphabet
pub fn generate_custom(length: usize, alphabet: &str) -> String {
    let mut output = String::with_capacity(length);
    
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_nanos();
    
    let mut seed = now as u64;
    let alphabet_len = alphabet.len();
    
    for _ in 0..length {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let index = (seed as usize) % alphabet_len;
        output.push(alphabet.chars().nth(index).unwrap_or('0'));
    }
    
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate() {
        let id = generate();
        assert_eq!(id.len(), 5);
    }

    #[test]
    fn test_generate_with_length() {
        let id = generate_with_length(10);
        assert_eq!(id.len(), 10);
    }

    #[test]
    fn test_generate_custom() {
        let alphabet = "abc";
        let id = generate_custom(5, alphabet);
        assert_eq!(id.len(), 5);
        for c in id.chars() {
            assert!(c == 'a' || c == 'b' || c == 'c');
        }
    }
    
    #[test]
    fn test_default_alphabet() {
        let id = generate();
        for c in id.chars() {
            assert!(ALPHABET.contains(c));
        }
    }
}
