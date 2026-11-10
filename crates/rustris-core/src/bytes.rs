use crate::{Error, Result};
use std::{ffi::OsString, os::unix::ffi::OsStringExt, path::PathBuf};

/// Valid OS bytes. Unicode labels are never used to reconstruct this value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OsBytes(Vec<u8>);

const MAX_VALUE_BYTES: usize = 64 * 1024;

impl OsBytes {
    /// Validate a Linux argument/path/environment value without changing its bytes.
    pub fn new(value: impl Into<Vec<u8>>) -> Result<Self> {
        let value = value.into();
        if value.len() > MAX_VALUE_BYTES {
            return Err(Error::Invalid(
                "A single OS value exceeds the 64 KiB profile limit.".into(),
            ));
        }
        if value.contains(&0) {
            return Err(Error::Invalid("OS values cannot contain NUL bytes.".into()));
        }
        Ok(Self(value))
    }

    /// Read exact bytes for storage or process construction.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Convert directly to an OS path, without a Unicode intermediate.
    pub fn path(&self) -> PathBuf {
        PathBuf::from(OsString::from_vec(self.0.clone()))
    }

    /// A readable label only; invalid UTF-8 is displayed with reversible escapes.
    pub fn label(&self) -> String {
        if self.needs_byte_editor() {
            self.escaped()
        } else {
            String::from_utf8_lossy(&self.0).into_owned()
        }
    }

    /// Whether ordinary Unicode editing can represent the exact value.
    pub fn is_utf8(&self) -> bool {
        std::str::from_utf8(&self.0).is_ok()
    }

    pub fn needs_byte_editor(&self) -> bool {
        std::str::from_utf8(&self.0).map_or(true, |text| text.chars().any(char::is_control))
    }

    /// A lossless explicit-byte editing representation. Backslashes are escaped.
    pub fn escaped(&self) -> String {
        let mut out = String::new();
        let mut remaining = self.0.as_slice();
        while !remaining.is_empty() {
            match std::str::from_utf8(remaining) {
                Ok(text) => {
                    push_text(&mut out, text);
                    break;
                }
                Err(error) => {
                    let valid = error.valid_up_to();
                    if let Ok(text) = std::str::from_utf8(&remaining[..valid]) {
                        push_text(&mut out, text);
                    }
                    let invalid = error.error_len().unwrap_or(remaining.len() - valid);
                    for byte in &remaining[valid..valid + invalid] {
                        out.push_str(&format!("\\x{byte:02x}"));
                    }
                    remaining = &remaining[valid + invalid..];
                }
            }
        }
        out
    }

    /// Parse user text in ordinary Unicode mode or explicit byte mode.
    pub fn from_editor(value: &str, exact_bytes: bool) -> Result<Self> {
        if !exact_bytes {
            return Self::new(value.as_bytes());
        }
        let mut out = Vec::new();
        let mut input = value.as_bytes().iter().copied();
        while let Some(byte) = input.next() {
            if byte != b'\\' {
                out.push(byte);
                continue;
            }
            match input.next() {
                Some(b'\\') => out.push(b'\\'),
                Some(b'x') => {
                    let high = input.next().and_then(hex_digit);
                    let low = input.next().and_then(hex_digit);
                    let (Some(high), Some(low)) = (high, low) else {
                        return Err(Error::Invalid(
                            "Exact byte mode needs \\x followed by two hex digits.".into(),
                        ));
                    };
                    out.push(high * 16 + low);
                }
                _ => {
                    return Err(Error::Invalid(
                        "Exact byte mode accepts \\\\ or \\xHH escapes.".into(),
                    ));
                }
            }
        }
        Self::new(out)
    }
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn push_text(out: &mut String, text: &str) {
    for ch in text.chars() {
        if ch == '\\' {
            out.push_str("\\\\");
        } else if ch.is_control() {
            for byte in ch.to_string().as_bytes() {
                out.push_str(&format!("\\x{byte:02x}"));
            }
        } else {
            out.push(ch);
        }
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "Explicit fixture assertions")]
mod tests {
    use super::*;

    #[test]
    fn every_non_nul_byte_round_trips_through_editor() {
        let original = OsBytes::new((1..=255).collect::<Vec<u8>>()).unwrap();
        assert_eq!(
            OsBytes::from_editor(&original.escaped(), true).unwrap(),
            original
        );
    }

    #[test]
    fn unicode_and_literal_backslashes_are_lossless() {
        let original = OsBytes::new("/Games/🦀\\name\n".as_bytes()).unwrap();
        assert_eq!(
            OsBytes::from_editor(&original.escaped(), true).unwrap(),
            original
        );
        assert_eq!(
            OsBytes::from_editor("C:\\Games", false).unwrap().as_bytes(),
            b"C:\\Games"
        );
    }

    #[test]
    fn malformed_escapes_and_nul_are_rejected() {
        for input in ["\\", "\\q", "\\xf", "\\xgg", "\\x00"] {
            assert!(OsBytes::from_editor(input, true).is_err(), "{input}");
        }
    }
}
