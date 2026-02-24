use std::collections::HashSet;

use crate::format::ident::encode_identifier;
use crate::refusal::payload::RefusalPayload;

pub const EMPTY_HEADER_PREFIX: &str = "__shape_col_";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeaderNormalizationError {
    Duplicate(Vec<u8>),
}

pub fn ascii_trim(bytes: &[u8]) -> &[u8] {
    let mut start = 0;
    let mut end = bytes.len();

    while start < end && (bytes[start] == b' ' || bytes[start] == b'\t') {
        start += 1;
    }
    while end > start && (bytes[end - 1] == b' ' || bytes[end - 1] == b'\t') {
        end -= 1;
    }
    &bytes[start..end]
}

pub fn normalize_headers(
    raw_headers: &[Vec<u8>],
) -> Result<Vec<Vec<u8>>, HeaderNormalizationError> {
    let mut seen = HashSet::with_capacity(raw_headers.len());
    let mut normalized = Vec::with_capacity(raw_headers.len());

    for (index, header) in raw_headers.iter().enumerate() {
        let trimmed = ascii_trim(header);
        let value = if trimmed.is_empty() {
            format!("{EMPTY_HEADER_PREFIX}{}", index + 1).into_bytes()
        } else {
            trimmed.to_vec()
        };
        if !seen.insert(value.clone()) {
            return Err(HeaderNormalizationError::Duplicate(value));
        }
        normalized.push(value);
    }

    Ok(normalized)
}

/// Normalize headers and map parser-visible failures to `E_HEADERS`.
pub fn normalize_headers_or_refusal(
    file: &str,
    raw_headers: &[Vec<u8>],
) -> Result<Vec<Vec<u8>>, RefusalPayload> {
    if raw_headers.is_empty() {
        return Err(RefusalPayload::headers_missing(file.to_owned()));
    }

    normalize_headers(raw_headers).map_err(|error| match error {
        HeaderNormalizationError::Duplicate(name) => {
            RefusalPayload::headers_duplicate(file.to_owned(), encode_identifier(&name))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{ascii_trim, normalize_headers, normalize_headers_or_refusal};
    use crate::normalize::headers::{EMPTY_HEADER_PREFIX, HeaderNormalizationError};
    use crate::refusal::codes::RefusalCode;

    #[test]
    fn ascii_trim_removes_only_ascii_space_and_tab() {
        assert_eq!(ascii_trim(b" \tloan_id\t "), b"loan_id");
        assert_eq!(ascii_trim(b"\nloan_id\n"), b"\nloan_id\n");
    }

    #[test]
    fn normalize_headers_trims_and_replaces_empty_names() {
        let raw = vec![
            b" loan_id ".to_vec(),
            b"\t\t".to_vec(),
            b"amount".to_vec(),
            b"".to_vec(),
        ];

        let normalized = normalize_headers(&raw).expect("headers should normalize");
        assert_eq!(normalized[0], b"loan_id");
        assert_eq!(
            normalized[1],
            format!("{EMPTY_HEADER_PREFIX}2").as_bytes().to_vec()
        );
        assert_eq!(normalized[2], b"amount");
        assert_eq!(
            normalized[3],
            format!("{EMPTY_HEADER_PREFIX}4").as_bytes().to_vec()
        );
    }

    #[test]
    fn normalize_headers_rejects_duplicates_after_normalization() {
        let raw = vec![b" loan_id ".to_vec(), b"loan_id".to_vec()];

        let error = normalize_headers(&raw).expect_err("duplicate should fail");
        assert_eq!(
            error,
            HeaderNormalizationError::Duplicate(b"loan_id".to_vec())
        );
    }

    #[test]
    fn normalize_headers_or_refusal_maps_missing_header_case() {
        let refusal = normalize_headers_or_refusal("old.csv", &[])
            .expect_err("empty header set should map to refusal");

        assert_eq!(refusal.code, RefusalCode::EHeaders);
        assert_eq!(refusal.detail["file"].as_str(), Some("old.csv"));
        assert_eq!(refusal.detail["issue"].as_str(), Some("missing"));
    }

    #[test]
    fn normalize_headers_or_refusal_maps_duplicate_with_encoded_name() {
        let raw = vec![b" amount ".to_vec(), b"amount".to_vec()];
        let refusal = normalize_headers_or_refusal("new.csv", &raw)
            .expect_err("duplicate should map to refusal");

        assert_eq!(refusal.code, RefusalCode::EHeaders);
        assert_eq!(refusal.detail["file"].as_str(), Some("new.csv"));
        assert_eq!(refusal.detail["issue"].as_str(), Some("duplicate"));
        assert_eq!(refusal.detail["name"].as_str(), Some("u8:amount"));
    }
}
