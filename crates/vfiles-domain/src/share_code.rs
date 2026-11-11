const SHARE_CODE_ALPHABET: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";

/// 对外分享码长度。8 位 base32 只有 40 位熵；在可被分布式猜解的分享链接上
/// 提高到 16 位（约 74 位有效熵，含 UUIDv4 固定位），旧 8 位链接仍可查询。
pub const SHARE_CODE_LENGTH: usize = 16;

/// Generate the short code exposed in share URLs and management screens.
pub fn generate_short_share_code() -> String {
    encode_share_code(uuid::Uuid::new_v4().into_bytes())
}

fn encode_share_code(random_bytes: [u8; 16]) -> String {
    let mut value = 0_u128;
    // Use the ten bytes after the UUIDv4 version field. The version nibble and
    // variant bits make a few bits fixed, but the remaining ~74 random bits are
    // still far beyond the old 40-bit code.
    for byte in &random_bytes[6..] {
        value = (value << 8) | u128::from(*byte);
    }

    let mut code = String::with_capacity(SHARE_CODE_LENGTH);
    for shift in (0..SHARE_CODE_LENGTH as u32).rev() {
        let index = ((value >> (shift * 5)) & 0x1f) as usize;
        code.push(char::from(SHARE_CODE_ALPHABET[index]));
    }
    code
}

#[cfg(test)]
mod tests {
    use super::{
        SHARE_CODE_ALPHABET, SHARE_CODE_LENGTH, encode_share_code, generate_short_share_code,
    };

    #[test]
    fn generated_share_codes_are_base32_with_the_expected_length() {
        let code = generate_short_share_code();

        assert_eq!(code.len(), SHARE_CODE_LENGTH);
        assert!(code.bytes().all(|byte| SHARE_CODE_ALPHABET.contains(&byte)));
    }

    #[test]
    fn share_codes_change_when_any_random_suffix_byte_changes() {
        let mut first = [0_u8; 16];
        first[6] = 0x40;
        first[8] = 0x80;
        first[11..].copy_from_slice(&[1, 2, 3, 4, 5]);

        let mut different_suffix = first;
        different_suffix[15] = 6;

        assert_ne!(
            encode_share_code(first),
            encode_share_code(different_suffix)
        );
    }
}
