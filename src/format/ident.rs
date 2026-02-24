const HEX: &[u8; 16] = b"0123456789abcdef";

pub fn encode_identifier(bytes: &[u8]) -> String {
    if let Ok(utf8) = std::str::from_utf8(bytes)
        && !bytes.iter().any(|&b| b <= 0x1f || b == 0x7f)
    {
        return format!("u8:{utf8}");
    }

    let mut out = String::with_capacity(4 + (bytes.len() * 2));
    out.push_str("hex:");
    for &byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::encode_identifier;

    #[test]
    fn encodes_utf8_without_controls_as_u8_prefix() {
        assert_eq!(encode_identifier(b"loan_id"), "u8:loan_id");
        assert_eq!(encode_identifier("cafe".as_bytes()), "u8:cafe");
    }

    #[test]
    fn encodes_non_utf8_or_control_bytes_as_hex_prefix() {
        assert_eq!(encode_identifier(b"loan\nid"), "hex:6c6f616e0a6964");
        assert_eq!(encode_identifier(&[0xff, 0x00, 0x41]), "hex:ff0041");
    }
}
