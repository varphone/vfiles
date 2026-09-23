use std::fmt::Write;
use std::time::{Duration, SystemTime};

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
    let mut range_values = headers.get_all(header::RANGE).iter();
    let Some(raw_value) = range_values.next() else {
        return RangeRequest::Full;
    };
    // Multiple field lines form a multi-range request. Since multipart range
    // responses are not implemented, ignore the entire Range header set.
    if range_values.next().is_some() {
        return RangeRequest::Full;
    }
    let Ok(raw_value) = raw_value.to_str() else {
        return RangeRequest::Full;
    };
    let Some((unit, spec)) = raw_value.trim().split_once('=') else {
        return RangeRequest::Full;
    };
    if !unit.eq_ignore_ascii_case("bytes") {
        return RangeRequest::Full;
    }
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

pub(crate) struct StreamingFileOptions<'a> {
    pub(crate) range_allowed: bool,
    pub(crate) request_headers: &'a HeaderMap,
    pub(crate) mime_type: Option<&'a str>,
    pub(crate) size_bytes: u64,
    pub(crate) attachment_filename: Option<&'a str>,
    pub(crate) etag: Option<&'a str>,
    pub(crate) modified_at: Option<time::OffsetDateTime>,
}

pub(crate) fn if_none_match_is_wildcard(headers: &HeaderMap) -> bool {
    let mut values = headers.get_all(header::IF_NONE_MATCH).iter();
    let Some(value) = values.next() else {
        return false;
    };
    values.next().is_none() && value.to_str().is_ok_and(|value| value.trim() == "*")
}

pub(crate) fn not_modified_response(
    etag: Option<&str>,
    modified_at: Option<time::OffsetDateTime>,
) -> ApiResult<Response> {
    let mut response = Response::new(Body::empty());
    *response.status_mut() = StatusCode::NOT_MODIFIED;
    insert_etag(response.headers_mut(), etag)?;
    insert_last_modified(response.headers_mut(), modified_at)?;
    Ok(response)
}

pub(crate) async fn streaming_file_response(
    mut reader: Box<dyn ReadSeek + Send + Unpin>,
    options: StreamingFileOptions<'_>,
) -> ApiResult<Response> {
    let StreamingFileOptions {
        range_allowed,
        request_headers,
        mime_type,
        size_bytes,
        attachment_filename,
        etag,
        modified_at,
    } = options;
    if request_headers.contains_key(header::IF_MATCH) && !if_match(request_headers, etag) {
        return precondition_failed(etag, modified_at);
    }
    if !request_headers.contains_key(header::IF_MATCH)
        && modified_at
            .is_some_and(|modified_at| if_unmodified_since_failed(request_headers, modified_at))
    {
        return precondition_failed(etag, modified_at);
    }
    if if_none_match(request_headers, etag) {
        return not_modified_response(etag, modified_at);
    }
    if !request_headers.contains_key(header::IF_NONE_MATCH)
        && modified_at.is_some_and(|modified_at| if_modified_since(request_headers, modified_at))
    {
        return not_modified_response(etag, modified_at);
    }

    let content_type = HeaderValue::from_str(mime_type.unwrap_or("application/octet-stream"))
        .map_err(|e| ApiError::Internal(format!("Invalid content type header: {}", e)))?;
    let accept_ranges = HeaderValue::from_static("bytes");
    let range_is_current = range_allowed
        && if request_headers.contains_key(header::IF_RANGE) {
            if_range_matches(request_headers, etag, modified_at)
        } else {
            range_allowed
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
            insert_last_modified(response.headers_mut(), modified_at)?;
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
            insert_last_modified(headers, modified_at)?;
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
            insert_last_modified(headers, modified_at)?;
            insert_content_length(headers, size_bytes)?;
            if let Some(filename) = attachment_filename {
                headers.insert(header::CONTENT_DISPOSITION, attachment_header(filename)?);
            }
            Ok(response)
        }
    }
}

fn precondition_failed(
    etag: Option<&str>,
    modified_at: Option<time::OffsetDateTime>,
) -> ApiResult<Response> {
    let mut response = Response::new(Body::empty());
    *response.status_mut() = StatusCode::PRECONDITION_FAILED;
    insert_etag(response.headers_mut(), etag)?;
    insert_last_modified(response.headers_mut(), modified_at)?;
    Ok(response)
}

fn insert_etag(headers: &mut HeaderMap, etag: Option<&str>) -> ApiResult<()> {
    if let Some(etag) = etag {
        let value = HeaderValue::from_str(etag)
            .map_err(|e| ApiError::Internal(format!("Invalid ETag header: {}", e)))?;
        headers.insert(header::ETAG, value);
    }
    Ok(())
}

fn insert_last_modified(
    headers: &mut HeaderMap,
    modified_at: Option<time::OffsetDateTime>,
) -> ApiResult<()> {
    if let Some(modified_at) = modified_at {
        let seconds = modified_at.unix_timestamp();
        let modified = SystemTime::UNIX_EPOCH
            .checked_add(Duration::from_secs(seconds.max(0) as u64))
            .unwrap_or(SystemTime::UNIX_EPOCH);
        let value = HeaderValue::from_str(&httpdate::fmt_http_date(modified))
            .map_err(|e| ApiError::Internal(format!("Invalid Last-Modified header: {e}")))?;
        headers.insert(header::LAST_MODIFIED, value);
    }
    Ok(())
}

fn if_modified_since(headers: &HeaderMap, modified_at: time::OffsetDateTime) -> bool {
    let Some(since) = single_header(headers, header::IF_MODIFIED_SINCE)
        .and_then(|value| httpdate::parse_http_date(value).ok())
    else {
        return false;
    };
    let modified_seconds = modified_at.unix_timestamp();
    if modified_seconds < 0 {
        return false;
    }
    let modified = SystemTime::UNIX_EPOCH + Duration::from_secs(modified_seconds as u64);
    modified <= since
}

fn if_unmodified_since_failed(headers: &HeaderMap, modified_at: time::OffsetDateTime) -> bool {
    let Some(date) = single_header(headers, header::IF_UNMODIFIED_SINCE)
        .and_then(|value| httpdate::parse_http_date(value).ok())
    else {
        return false;
    };
    let modified_seconds = modified_at.unix_timestamp();
    if modified_seconds < 0 {
        return false;
    }
    let modified = SystemTime::UNIX_EPOCH + Duration::from_secs(modified_seconds as u64);
    modified > date
}

fn if_none_match(headers: &HeaderMap, current_etag: Option<&str>) -> bool {
    headers
        .get_all(header::IF_NONE_MATCH)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .any(|value| {
            any_etag_candidate(value, |candidate| {
                candidate == "*"
                    || current_etag.is_some_and(|etag| weak_etag_eq(candidate.trim(), etag))
            })
        })
}

fn single_header(headers: &HeaderMap, name: header::HeaderName) -> Option<&str> {
    let mut values = headers.get_all(name).iter();
    let value = values.next()?;
    if values.next().is_some() {
        return None;
    }
    value.to_str().ok()
}

fn if_match(headers: &HeaderMap, current_etag: Option<&str>) -> bool {
    headers
        .get_all(header::IF_MATCH)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .any(|value| {
            any_etag_candidate(value, |candidate| {
                candidate == "*"
                    || (!candidate.starts_with("W/") && current_etag == Some(candidate))
            })
        })
}

fn any_etag_candidate(value: &str, mut matches: impl FnMut(&str) -> bool) -> bool {
    let mut start = 0;
    let mut in_quotes = false;
    for (index, byte) in value.bytes().enumerate() {
        if byte == b'"' {
            in_quotes = !in_quotes;
        } else if byte == b',' && !in_quotes {
            if matches(value[start..index].trim()) {
                return true;
            }
            start = index + 1;
        }
    }
    matches(value[start..].trim())
}

fn weak_etag_eq(candidate: &str, current_etag: &str) -> bool {
    let candidate = candidate.strip_prefix("W/").unwrap_or(candidate);
    candidate == current_etag
}

fn if_range_matches(
    headers: &HeaderMap,
    current_etag: Option<&str>,
    modified_at: Option<time::OffsetDateTime>,
) -> bool {
    if !headers.contains_key(header::IF_RANGE) {
        return true;
    }
    let Some(value) = single_header(headers, header::IF_RANGE) else {
        return false;
    };
    let value = value.trim();
    if value.starts_with('"') || value.starts_with("W/") {
        return current_etag.is_some_and(|etag| value == etag && !value.starts_with("W/"));
    }

    let (Some(modified_at), Ok(date)) = (modified_at, httpdate::parse_http_date(value)) else {
        return false;
    };
    let modified_seconds = modified_at.unix_timestamp();
    modified_seconds >= 0
        && date
            .duration_since(SystemTime::UNIX_EPOCH)
            .is_ok_and(|date| date.as_secs() == modified_seconds as u64)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_unit_is_case_insensitive() {
        for value in ["bytes=2-4", "Bytes=2-4", "BYTES=2-4"] {
            let mut headers = HeaderMap::new();
            headers.insert(header::RANGE, HeaderValue::from_str(value).unwrap());
            assert_eq!(
                parse_range(&headers, 10),
                RangeRequest::Partial { start: 2, end: 4 },
                "range value {value:?}"
            );
        }
    }

    #[test]
    fn repeated_range_fields_are_ignored_as_multi_range_requests() {
        let mut headers = HeaderMap::new();
        headers.append(header::RANGE, HeaderValue::from_static("bytes=0-0"));
        headers.append(header::RANGE, HeaderValue::from_static("bytes=2-2"));

        assert_eq!(parse_range(&headers, 10), RangeRequest::Full);
    }

    #[test]
    fn if_none_match_checks_every_repeated_header_field() {
        let mut headers = HeaderMap::new();
        headers.append(header::IF_NONE_MATCH, HeaderValue::from_static("\"older\""));
        headers.append(
            header::IF_NONE_MATCH,
            HeaderValue::from_static("W/\"current\", \"other\""),
        );

        assert!(if_none_match(&headers, Some("\"current\"")));
    }

    #[test]
    fn if_none_match_keeps_quoted_commas_inside_one_entity_tag() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::IF_NONE_MATCH,
            HeaderValue::from_static("\"older,version\", W/\"current\""),
        );

        assert!(if_none_match(&headers, Some("\"current\"")));
    }

    #[test]
    fn if_none_match_wildcard_matches_existing_representations_without_etags() {
        let mut headers = HeaderMap::new();
        headers.insert(header::IF_NONE_MATCH, HeaderValue::from_static("*"));
        assert!(if_none_match(&headers, None));

        headers.insert(
            header::IF_NONE_MATCH,
            HeaderValue::from_static("\"specific-tag\""),
        );
        assert!(!if_none_match(&headers, None));
    }

    #[test]
    fn if_match_uses_strong_comparison_and_accepts_existing_resource_wildcard() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::IF_MATCH,
            HeaderValue::from_static("\"older\", W/\"current\""),
        );
        assert!(!if_match(&headers, Some("\"current\"")));

        headers.insert(
            header::IF_MATCH,
            HeaderValue::from_static("\"older,version\", \"current\""),
        );
        assert!(if_match(&headers, Some("\"current\"")));

        headers.insert(header::IF_MATCH, HeaderValue::from_static("*"));
        assert!(if_match(&headers, None));
    }

    #[test]
    fn if_unmodified_since_fails_only_when_the_representation_is_newer() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::IF_UNMODIFIED_SINCE,
            HeaderValue::from_static("Thu, 01 Jan 1970 00:00:05 GMT"),
        );
        let modified = time::OffsetDateTime::UNIX_EPOCH + time::Duration::seconds(10);
        assert!(if_unmodified_since_failed(&headers, modified));

        headers.insert(
            header::IF_UNMODIFIED_SINCE,
            HeaderValue::from_static("Thu, 01 Jan 1970 00:00:10 GMT"),
        );
        assert!(!if_unmodified_since_failed(&headers, modified));
    }

    #[test]
    fn duplicate_date_preconditions_are_ignored() {
        let modified = time::OffsetDateTime::UNIX_EPOCH + time::Duration::seconds(10);
        let mut headers = HeaderMap::new();
        headers.append(
            header::IF_MODIFIED_SINCE,
            HeaderValue::from_static("Thu, 01 Jan 1970 00:00:20 GMT"),
        );
        headers.append(
            header::IF_MODIFIED_SINCE,
            HeaderValue::from_static("Thu, 01 Jan 1970 00:00:05 GMT"),
        );
        assert!(!if_modified_since(&headers, modified));

        headers.remove(header::IF_MODIFIED_SINCE);
        headers.append(
            header::IF_UNMODIFIED_SINCE,
            HeaderValue::from_static("Thu, 01 Jan 1970 00:00:05 GMT"),
        );
        headers.append(
            header::IF_UNMODIFIED_SINCE,
            HeaderValue::from_static("Thu, 01 Jan 1970 00:00:20 GMT"),
        );
        assert!(!if_unmodified_since_failed(&headers, modified));
    }

    #[test]
    fn repeated_if_range_fields_cause_a_full_response() {
        let mut headers = HeaderMap::new();
        headers.append(header::IF_RANGE, HeaderValue::from_static("\"current\""));
        headers.append(
            header::IF_RANGE,
            HeaderValue::from_static("Thu, 01 Jan 1970 00:00:20 GMT"),
        );
        let modified = time::OffsetDateTime::UNIX_EPOCH + time::Duration::seconds(10);

        assert!(!if_range_matches(
            &headers,
            Some("\"current\""),
            Some(modified)
        ));
    }
}
