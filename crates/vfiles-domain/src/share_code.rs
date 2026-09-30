const SHARE_CODE_ALPHABET: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";

/// Generate the short code exposed in share URLs and management screens.
pub fn generate_short_share_code() -> String {
    encode_share_code(uuid::Uuid::new_v4().into_bytes())
}

fn encode_share_code(random_bytes: [u8; 16]) -> String {
    let mut value = 0_u64;
    // UUIDv4 reserves bits in bytes 6 and 8. The final five bytes are all
    // random, so the user-facing eight-character code retains all 40 bits.
    for byte in &random_bytes[11..] {
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
    use super::{SHARE_CODE_ALPHABET, encode_share_code, generate_short_share_code};

    #[test]
    fn generated_share_codes_are_eight_base32_characters() {
        let code = generate_short_share_code();

        assert_eq!(code.len(), 8);
        assert!(code.bytes().all(|byte| SHARE_CODE_ALPHABET.contains(&byte)));
    }

    #[test]
    fn share_codes_use_the_five_unrestricted_uuid_bytes() {
        let mut first = [0_u8; 16];
        first[0] = 1;
        first[11..].copy_from_slice(&[1, 2, 3, 4, 5]);

        let mut same_random_suffix = [0_u8; 16];
        same_random_suffix[6] = 0x40;
        same_random_suffix[8] = 0x80;
        same_random_suffix[11..].copy_from_slice(&[1, 2, 3, 4, 5]);

        let mut different_suffix = same_random_suffix;
        different_suffix[15] = 6;

        assert_eq!(
            encode_share_code(first),
            encode_share_code(same_random_suffix)
        );
        assert_ne!(
            encode_share_code(same_random_suffix),
            encode_share_code(different_suffix)
        );
    }
}
