use std::fmt::Write;

use axum::{
    body::Body,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::Response,
};
use tokio::io::{AsyncReadExt, AsyncSeekExt, SeekFrom};
use tokio_util::io::ReaderStream;
use vfiles_domain::ReadSeek;

use crate::error::{ApiError, ApiResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RangeRequest {
    Full,
    Partial { start: u64, end: u64 },
    Unsatisfiable,
}

pub(crate) fn parse_range(headers: &HeaderMap, total_size: u64) -> RangeRequest {
    let Some(raw_value) = headers.get(header::RANGE) else {
        return RangeRequest::Full;
    };
    let Ok(raw_value) = raw_value.to_str() else {
        return RangeRequest::Full;
    };
    let Some(spec) = raw_value.trim().strip_prefix("bytes=") else {
        return RangeRequest::Full;
    };
    if spec.contains(',') {
        // Multiple ranges are intentionally not supported; serve the full file.
        return RangeRequest::Full;
    }
    let Some((start_raw, end_raw)) = spec.split_once('-') else {
        return RangeRequest::Full;
    };

    if total_size == 0 {
        return RangeRequest::Unsatisfiable;
    }

    if start_raw.is_empty() {
        let Ok(suffix_len) = end_raw.parse::<u64>() else {
            return RangeRequest::Full;
        };
        if suffix_len == 0 {
            return RangeRequest::Unsatisfiable;
        }
        let start = total_size.saturating_sub(suffix_len);
        return RangeRequest::Partial {
            start,
            end: total_size - 1,
        };
    }

    let Ok(start) = start_raw.parse::<u64>() else {
        return RangeRequest::Full;
    };
    if start >= total_size {
        return RangeRequest::Unsatisfiable;
    }

    let end = if end_raw.is_empty() {
        total_size - 1
    } else {
        let Ok(end) = end_raw.parse::<u64>() else {
            return RangeRequest::Full;
        };
        if end < start {
            return RangeRequest::Full;
        }
        end.min(total_size - 1)
    };

    RangeRequest::Partial { start, end }
}

pub(crate) fn content_range_value(start: u64, end: u64, total: u64) -> ApiResult<HeaderValue> {
    HeaderValue::from_str(&format!("bytes {}-{}/{}", start, end, total))
        .map_err(|e| ApiError::Internal(format!("Invalid content range header: {}", e)))
}

pub(crate) fn unsatisfied_content_range_value(total: u64) -> ApiResult<HeaderValue> {
    HeaderValue::from_str(&format!("bytes */{}", total))
        .map_err(|e| ApiError::Internal(format!("Invalid content range header: {}", e)))
}

pub(crate) async fn streaming_file_response(
    mut reader: Box<dyn ReadSeek + Send + Unpin>,
    request_headers: &HeaderMap,
    mime_type: Option<&str>,
    size_bytes: u64,
    attachment_filename: Option<&str>,
    etag: Option<&str>,
) -> ApiResult<Response> {
    if let Some(etag) = etag
        && if_none_match(request_headers, etag)
    {
        let mut response = Response::new(Body::empty());
        *response.status_mut() = StatusCode::NOT_MODIFIED;
        insert_etag(response.headers_mut(), Some(etag))?;
        return Ok(response);
    }

    let content_type = HeaderValue::from_str(mime_type.unwrap_or("application/octet-stream"))
        .map_err(|e| ApiError::Internal(format!("Invalid content type header: {}", e)))?;
    let accept_ranges = HeaderValue::from_static("bytes");
    let range_is_current = if request_headers.contains_key(header::IF_RANGE) {
        etag.is_some_and(|etag| if_range_matches(request_headers, etag))
    } else {
        true
    };
    let range = if range_is_current {
        parse_range(request_headers, size_bytes)
    } else {
        RangeRequest::Full
    };

    match range {
        RangeRequest::Unsatisfiable => {
            let mut response = Response::new(Body::empty());
            *response.status_mut() = StatusCode::RANGE_NOT_SATISFIABLE;
            response
                .headers_mut()
                .insert(header::ACCEPT_RANGES, accept_ranges);
            response.headers_mut().insert(
                header::CONTENT_RANGE,
                unsatisfied_content_range_value(size_bytes)?,
            );
            insert_etag(response.headers_mut(), etag)?;
            if let Some(filename) = attachment_filename {
                response
                    .headers_mut()
                    .insert(header::CONTENT_DISPOSITION, attachment_header(filename)?);
            }
            Ok(response)
        }
        RangeRequest::Partial { start, end } => {
            reader
                .seek(SeekFrom::Start(start))
                .await
                .map_err(|e| ApiError::Internal(format!("Failed to seek blob: {}", e)))?;

            let length = end - start + 1;
            let mut response =
                Response::new(Body::from_stream(ReaderStream::new(reader.take(length))));
            *response.status_mut() = StatusCode::PARTIAL_CONTENT;
            let headers = response.headers_mut();
            headers.insert(header::CONTENT_TYPE, content_type);
            headers.insert(header::ACCEPT_RANGES, accept_ranges);
            insert_etag(headers, etag)?;
            headers.insert(
                header::CONTENT_RANGE,
                content_range_value(start, end, size_bytes)?,
            );
            insert_content_length(headers, length)?;
            if let Some(filename) = attachment_filename {
                headers.insert(header::CONTENT_DISPOSITION, attachment_header(filename)?);
            }
            Ok(response)
        }
        RangeRequest::Full => {
            let mut response = Response::new(Body::from_stream(ReaderStream::new(reader)));
            let headers = response.headers_mut();
            headers.insert(header::CONTENT_TYPE, content_type);
            headers.insert(header::ACCEPT_RANGES, accept_ranges);
            insert_etag(headers, etag)?;
            insert_content_length(headers, size_bytes)?;
            if let Some(filename) = attachment_filename {
                headers.insert(header::CONTENT_DISPOSITION, attachment_header(filename)?);
            }
            Ok(response)
        }
    }
}

fn insert_etag(headers: &mut HeaderMap, etag: Option<&str>) -> ApiResult<()> {
    if let Some(etag) = etag {
        let value = HeaderValue::from_str(etag)
            .map_err(|e| ApiError::Internal(format!("Invalid ETag header: {}", e)))?;
        headers.insert(header::ETAG, value);
    }
    Ok(())
}

fn if_none_match(headers: &HeaderMap, current_etag: &str) -> bool {
    let Some(value) = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    let mut start = 0;
    let mut in_quotes = false;
    for (index, byte) in value.bytes().enumerate() {
        if byte == b'"' {
            in_quotes = !in_quotes;
        } else if byte == b',' && !in_quotes {
            if etag_candidate_matches(&value[start..index], current_etag) {
                return true;
            }
            start = index + 1;
        }
    }
    etag_candidate_matches(&value[start..], current_etag)
}

fn etag_candidate_matches(candidate: &str, current_etag: &str) -> bool {
    let candidate = candidate.trim();
    candidate == "*" || weak_etag_eq(candidate, current_etag)
}

fn weak_etag_eq(candidate: &str, current_etag: &str) -> bool {
    let candidate = candidate.strip_prefix("W/").unwrap_or(candidate);
    candidate == current_etag
}

fn if_range_matches(headers: &HeaderMap, current_etag: &str) -> bool {
    let Some(value) = headers
        .get(header::IF_RANGE)
        .and_then(|value| value.to_str().ok())
    else {
        return true;
    };
    // If-Range requires a strong entity-tag comparison. Date validators are not
    // emitted by this endpoint, so they cannot authorize a partial response.
    value.trim() == current_etag && !value.trim().starts_with("W/")
}

fn insert_content_length(headers: &mut axum::http::HeaderMap, size_bytes: u64) -> ApiResult<()> {
    let value = HeaderValue::from_str(&size_bytes.to_string())
        .map_err(|e| ApiError::Internal(format!("Invalid content length header: {}", e)))?;
    headers.insert(header::CONTENT_LENGTH, value);
    Ok(())
}

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
