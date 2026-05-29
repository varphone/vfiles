use std::fmt::Write;

use axum::http::HeaderValue;

use crate::error::{ApiError, ApiResult};

pub(crate) fn attachment_header(filename: &str) -> ApiResult<HeaderValue> {
    let fallback = ascii_filename_fallback(filename);
    let encoded = encode_rfc5987(filename);

    HeaderValue::from_str(&format!(
        "attachment; filename=\"{}\"; filename*=UTF-8''{}",
        fallback, encoded
    ))
    .map_err(|e| ApiError::Internal(format!("Invalid content disposition header: {}", e)))
}

fn ascii_filename_fallback(filename: &str) -> String {
    let fallback = filename
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '-' | '_' | ' ' => ch,
            _ => '_',
        })
        .collect::<String>();

    if fallback
        .trim_matches(|ch| ch == ' ' || ch == '_')
        .is_empty()
    {
        "download".to_string()
    } else {
        fallback
    }
}

fn encode_rfc5987(value: &str) -> String {
    let mut encoded = String::new();

    for &byte in value.as_bytes() {
        if is_rfc5987_attr_char(byte) {
            encoded.push(byte as char);
        } else {
            let _ = write!(&mut encoded, "%{:02X}", byte);
        }
    }

    encoded
}

fn is_rfc5987_attr_char(byte: u8) -> bool {
    matches!(
        byte,
        b'0'..=b'9'
            | b'A'..=b'Z'
            | b'a'..=b'z'
            | b'!'
            | b'#'
            | b'$'
            | b'&'
            | b'+'
            | b'-'
            | b'.'
            | b'^'
            | b'_'
            | b'`'
            | b'|'
            | b'~'
    )
}
