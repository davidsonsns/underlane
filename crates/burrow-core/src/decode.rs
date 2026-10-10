//! Display-time decoding of captured bodies (stored raw, as they went over the wire).

use std::io::Read;

/// Result of decoding a captured body for display.
#[derive(Debug, Clone, PartialEq)]
pub struct Decoded {
    pub bytes: Vec<u8>,
    /// The `Content-Encoding` that was undone, if any.
    pub decoded_from: Option<String>,
    /// Decoding stopped early (truncated capture or corrupt stream); `bytes` is the decodable prefix.
    pub partial: bool,
}

/// Undo `Content-Encoding` (gzip, x-gzip, deflate, br). Unknown encodings are returned as-is.
/// Truncated streams decode as far as possible.
pub fn decode_body(raw: &[u8], content_encoding: Option<&str>) -> Decoded {
    let enc = content_encoding.map(|e| e.trim().to_ascii_lowercase()).unwrap_or_default();
    let identity = || Decoded { bytes: raw.to_vec(), decoded_from: None, partial: false };
    if raw.is_empty() || enc.is_empty() || enc == "identity" {
        return identity();
    }
    let reader: Box<dyn Read + '_> = match enc.as_str() {
        "gzip" | "x-gzip" => Box::new(flate2::read::MultiGzDecoder::new(raw)),
        "deflate" => {
            // "deflate" is zlib-wrapped per spec, but raw deflate is common in the wild.
            if raw.len() >= 2 && (u16::from(raw[0]) << 8 | u16::from(raw[1])) % 31 == 0 && raw[0] & 0x0f == 8 {
                Box::new(flate2::read::ZlibDecoder::new(raw))
            } else {
                Box::new(flate2::read::DeflateDecoder::new(raw))
            }
        }
        "br" => Box::new(brotli_decompressor::Decompressor::new(raw, 4096)),
        _ => return identity(),
    };
    let (bytes, partial) = read_tolerant(reader);
    if bytes.is_empty() && partial {
        return Decoded { bytes: raw.to_vec(), decoded_from: None, partial: true };
    }
    Decoded { bytes, decoded_from: Some(enc), partial }
}

/// Read until EOF or the first error, keeping everything decoded so far.
fn read_tolerant(mut r: impl Read) -> (Vec<u8>, bool) {
    let mut out = Vec::new();
    let mut buf = [0u8; 16 * 1024];
    loop {
        match r.read(&mut buf) {
            Ok(0) => return (out, false),
            Ok(n) => {
                out.extend_from_slice(&buf[..n]);
                if out.len() > 64 * 1024 * 1024 {
                    return (out, true);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return (out, true),
        }
    }
}

/// Classify bytes for display.
pub fn is_probably_text(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return true;
    }
    let sample = &bytes[..bytes.len().min(4096)];
    match std::str::from_utf8(sample) {
        Ok(s) => !s.chars().any(|c| c == '\0' || (c.is_control() && !matches!(c, '\n' | '\r' | '\t' | '\u{c}'))),
        // A multi-byte char cut at the sample boundary is still text.
        Err(e) => e.valid_up_to() + 4 >= sample.len() && e.error_len().is_none(),
    }
}
