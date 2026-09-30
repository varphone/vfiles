const SHARE_CODE_ALPHABET: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";

/// Generate the short code exposed in share URLs and management screens.
pub fn generate_short_share_code() -> String {
    let random_bytes = uuid::Uuid::new_v4().into_bytes();
    let mut value = 0_u64;
    for byte in random_bytes.iter().take(5) {
        value = (value << 8) | u64::from(*byte);
    }

    let mut code = String::with_capacity(8);
    for shift in (0..8_u32).rev() {
        let index = ((value >> (shift * 5)) & 0x1f) as usize;
        code.push(char::from(SHARE_CODE_ALPHABET[index]));
    }
    code
}

#[cfg(test)]
mod tests {
    use super::{SHARE_CODE_ALPHABET, generate_short_share_code};

    #[test]
    fn generated_share_codes_are_eight_base32_characters() {
        let code = generate_short_share_code();

        assert_eq!(code.len(), 8);
        assert!(code.bytes().all(|byte| SHARE_CODE_ALPHABET.contains(&byte)));
    }
}
