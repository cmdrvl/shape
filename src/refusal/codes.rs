use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum RefusalCode {
    #[serde(rename = "E_IO")]
    EIo,
    #[serde(rename = "E_ENCODING")]
    EEncoding,
    #[serde(rename = "E_CSV_PARSE")]
    ECsvParse,
    #[serde(rename = "E_EMPTY")]
    EEmpty,
    #[serde(rename = "E_HEADERS")]
    EHeaders,
    #[serde(rename = "E_DIALECT")]
    EDialect,
    #[serde(rename = "E_AMBIGUOUS_PROFILE")]
    EAmbiguousProfile,
    #[serde(rename = "E_INPUT_NOT_LOCKED")]
    EInputNotLocked,
    #[serde(rename = "E_INPUT_DRIFT")]
    EInputDrift,
    #[serde(rename = "E_TOO_LARGE")]
    ETooLarge,
}

impl RefusalCode {
    pub const ALL: [Self; 10] = [
        Self::EIo,
        Self::EEncoding,
        Self::ECsvParse,
        Self::EEmpty,
        Self::EHeaders,
        Self::EDialect,
        Self::EAmbiguousProfile,
        Self::EInputNotLocked,
        Self::EInputDrift,
        Self::ETooLarge,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::EIo => "E_IO",
            Self::EEncoding => "E_ENCODING",
            Self::ECsvParse => "E_CSV_PARSE",
            Self::EEmpty => "E_EMPTY",
            Self::EHeaders => "E_HEADERS",
            Self::EDialect => "E_DIALECT",
            Self::EAmbiguousProfile => "E_AMBIGUOUS_PROFILE",
            Self::EInputNotLocked => "E_INPUT_NOT_LOCKED",
            Self::EInputDrift => "E_INPUT_DRIFT",
            Self::ETooLarge => "E_TOO_LARGE",
        }
    }

    pub fn reason(self) -> &'static str {
        match self {
            Self::EIo => "Can't read file",
            Self::EEncoding => "Unsupported encoding (UTF-16/32 BOM or NUL bytes)",
            Self::ECsvParse => "Can't parse as CSV",
            Self::EEmpty => "One or both files empty (no data rows after header)",
            Self::EHeaders => "Missing header or duplicate headers",
            Self::EDialect => "Delimiter ambiguous or undetectable",
            Self::EAmbiguousProfile => "Both --profile and --profile-id were provided",
            Self::EInputNotLocked => "Input file not present in any provided lockfile",
            Self::EInputDrift => "Input file hash doesn't match the referenced lock member",
            Self::ETooLarge => "Input exceeds --max-rows or --max-bytes",
        }
    }

    pub fn supports_next_command(self) -> bool {
        matches!(self, Self::EDialect | Self::ETooLarge)
    }
}

#[cfg(test)]
mod tests {
    use super::RefusalCode;

    #[test]
    fn serializes_uppercase_code_names() {
        let encoded = serde_json::to_string(&RefusalCode::ALL).expect("serialize refusal codes");

        assert_eq!(
            encoded,
            "[\"E_IO\",\"E_ENCODING\",\"E_CSV_PARSE\",\"E_EMPTY\",\"E_HEADERS\",\"E_DIALECT\",\"E_AMBIGUOUS_PROFILE\",\"E_INPUT_NOT_LOCKED\",\"E_INPUT_DRIFT\",\"E_TOO_LARGE\"]"
        );
    }

    #[test]
    fn every_code_has_non_empty_reason() {
        for code in RefusalCode::ALL {
            assert!(
                !code.reason().trim().is_empty(),
                "missing reason for {}",
                code.as_str()
            );
        }
    }

    #[test]
    fn marks_only_actionable_codes_for_next_command() {
        assert!(!RefusalCode::EIo.supports_next_command());
        assert!(!RefusalCode::EEncoding.supports_next_command());
        assert!(!RefusalCode::ECsvParse.supports_next_command());
        assert!(!RefusalCode::EEmpty.supports_next_command());
        assert!(!RefusalCode::EHeaders.supports_next_command());
        assert!(RefusalCode::EDialect.supports_next_command());
        assert!(!RefusalCode::EAmbiguousProfile.supports_next_command());
        assert!(!RefusalCode::EInputNotLocked.supports_next_command());
        assert!(!RefusalCode::EInputDrift.supports_next_command());
        assert!(RefusalCode::ETooLarge.supports_next_command());
    }
}
