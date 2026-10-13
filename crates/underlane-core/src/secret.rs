//! Secrets are exposed only at the boundary that needs their contents.

use std::fmt;

use zeroize::Zeroize;

pub struct Secret<T: Zeroize>(T);

impl<T: Zeroize> Secret<T> {
    pub fn new(value: T) -> Self {
        Self(value)
    }

    pub fn expose(&self) -> &T {
        &self.0
    }

    pub(crate) fn expose_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T: Zeroize + Clone> Clone for Secret<T> {
    fn clone(&self) -> Self {
        Self::new(self.0.clone())
    }
}

impl<T: Zeroize> Drop for Secret<T> {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl<T: Zeroize> fmt::Debug for Secret<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret([REDACTED])")
    }
}

impl<T: Zeroize> fmt::Display for Secret<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

const REDACTED: &str = "[REDACTED]";

fn secret_key(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().as_str(),
        "token"
            | "apitoken"
            | "accounttag"
            | "tunnelsecret"
            | "credentials-contents"
            | "tunnel_token"
            | "tunnel_cred_contents"
            | "tunnel_credentials_contents"
            | "callback"
            | "callbackurl"
    )
}

fn secret_flag(flag: &str) -> bool {
    matches!(flag, "--token" | "--credentials-contents")
}

/// Sanitize argv, diagnostic text and JSON before it can reach logs or errors.
/// An unterminated secret value consumes the rest of the input rather than leaking a tail.
pub fn redact_text(text: &str) -> String {
    if let Ok(mut json) = serde_json::from_str::<serde_json::Value>(text) {
        if redact_json(&mut json) {
            return json.to_string();
        }
    }
    // Debug/partial JSON can retain escaped quotes without being a valid JSON document.
    if text.contains("\\\"") {
        let unescaped = text.replace("\\\"", "\"");
        let redacted = redact_flat(&unescaped);
        if redacted != unescaped {
            return redacted;
        }
    }
    redact_flat(text)
}

fn redact_json(value: &mut serde_json::Value) -> bool {
    let mut changed = false;
    match value {
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                if secret_key(key) {
                    changed |= value.as_str() != Some(REDACTED);
                    *value = serde_json::Value::String(REDACTED.into());
                } else {
                    changed |= redact_json(value);
                }
            }
        }
        serde_json::Value::Array(values) => {
            let mut next_secret = false;
            for value in values {
                if next_secret {
                    changed |= value.as_str() != Some(REDACTED);
                    *value = serde_json::Value::String(REDACTED.into());
                    next_secret = false;
                } else {
                    next_secret = value.as_str().map(secret_flag).unwrap_or(false);
                    changed |= redact_json(value);
                }
            }
        }
        serde_json::Value::String(text) => {
            let redacted = redact_flat(text);
            changed = *text != redacted;
            *text = redacted;
        }
        _ => {}
    }
    changed
}

fn boundary(byte: u8) -> bool {
    byte.is_ascii_whitespace() || matches!(byte, b'\'' | b'"' | b'[' | b'{' | b',' | b':' | b'?' | b'&')
}

fn redact_flat(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut copied = 0;
    let mut i = 0;
    while i < bytes.len() {
        if i != 0 && !boundary(bytes[i - 1]) {
            i += 1;
            continue;
        }
        let Some(remaining) = text.get(i..) else {
            i += 1;
            continue;
        };
        let marker =
            ["--credentials-contents", "--token", "TUNNEL_CREDENTIALS_CONTENTS", "TUNNEL_CRED_CONTENTS", "TUNNEL_TOKEN", "callback"]
                .into_iter()
                .find(|marker| remaining.starts_with(marker));
        let credentials = marker.map(|marker| marker.contains("credentials") || marker.contains("CRED")).unwrap_or(false);
        let mut json_key = false;
        let start = if let Some(marker) = marker {
            let mut pos = i + marker.len();
            if bytes.get(pos) == Some(&b'=') {
                pos += 1;
            } else if marker.starts_with("--") && i > 0 && matches!(bytes[i - 1], b'\'' | b'"') && bytes.get(pos) == Some(&bytes[i - 1]) {
                // Quoted argv in Debug output or incomplete JSON arrays.
                pos += 1;
                while bytes.get(pos).map(u8::is_ascii_whitespace).unwrap_or(false) {
                    pos += 1;
                }
                if bytes.get(pos) == Some(&b',') {
                    pos += 1;
                }
                while bytes.get(pos).map(u8::is_ascii_whitespace).unwrap_or(false) {
                    pos += 1;
                }
            } else if marker.starts_with("--") && bytes.get(pos).map(u8::is_ascii_whitespace).unwrap_or(false) {
                while bytes.get(pos).map(u8::is_ascii_whitespace).unwrap_or(false) {
                    pos += 1;
                }
            } else {
                i += 1;
                continue;
            }
            pos
        } else if bytes[i] == b'"' {
            let Some(end) = remaining[1..].find('"').map(|offset| i + offset + 1) else {
                i += 1;
                continue;
            };
            if !secret_key(&text[i + 1..end]) {
                i += 1;
                continue;
            }
            let mut pos = end + 1;
            while bytes.get(pos).map(u8::is_ascii_whitespace).unwrap_or(false) {
                pos += 1;
            }
            if bytes.get(pos) != Some(&b':') {
                i += 1;
                continue;
            }
            pos += 1;
            while bytes.get(pos).map(u8::is_ascii_whitespace).unwrap_or(false) {
                pos += 1;
            }
            json_key = true;
            pos
        } else {
            i += 1;
            continue;
        };
        let mut end = value_end(bytes, start);
        if credentials || json_key {
            let value = &text[start..end];
            let unquoted = value.strip_prefix('\'').and_then(|value| value.strip_suffix('\'')).unwrap_or(value);
            if serde_json::from_str::<serde_json::Value>(unquoted).is_err() {
                end = bytes.len();
            }
        }
        out.push_str(&text[copied..start]);
        if json_key {
            out.push('"');
        }
        out.push_str(REDACTED);
        if json_key {
            out.push('"');
        }
        copied = end;
        i = end;
    }
    out.push_str(&text[copied..]);
    out
}

fn value_end(bytes: &[u8], start: usize) -> usize {
    let Some(&first) = bytes.get(start) else { return bytes.len() };
    if matches!(first, b'\'' | b'"') {
        let mut escaped = false;
        for (offset, &byte) in bytes[start + 1..].iter().enumerate() {
            if !escaped && byte == first {
                return start + offset + 2;
            }
            escaped = !escaped && byte == b'\\';
        }
        return bytes.len();
    }
    if matches!(first, b'{' | b'[') {
        let mut closes = Vec::new();
        let mut quoted = None;
        let mut escaped = false;
        for (offset, &byte) in bytes[start..].iter().enumerate() {
            if let Some(quote) = quoted {
                if !escaped && byte == quote {
                    quoted = None;
                }
                escaped = !escaped && byte == b'\\';
                continue;
            }
            match byte {
                b'\'' | b'"' => quoted = Some(byte),
                b'{' => closes.push(b'}'),
                b'[' => closes.push(b']'),
                b'}' | b']' => {
                    if closes.pop() != Some(byte) {
                        return bytes.len();
                    }
                    if closes.is_empty() {
                        return start + offset + 1;
                    }
                }
                _ => {}
            }
        }
        return bytes.len();
    }
    bytes[start..]
        .iter()
        .position(|byte| byte.is_ascii_whitespace() || matches!(byte, b',' | b'}' | b']'))
        .map(|offset| start + offset)
        .unwrap_or(bytes.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_debug_display_and_clone_do_not_expose_contents() {
        let secret = Secret::new("invented-run-token".to_string());
        assert_eq!(secret.expose(), "invented-run-token");
        assert_eq!(secret.clone().expose(), secret.expose());
        assert!(!format!("{secret:?} {secret}").contains("invented-run-token"));
        let spec = crate::supervisor::RunSpec {
            tunnel: crate::model::Tunnel {
                id: "test".into(),
                project_id: "test".into(),
                name: "Test".into(),
                kind: crate::model::TunnelKind::Token,
                config_path: None,
                origin_url: None,
                inspect: false,
                autostart: false,
                loglevel: "info".into(),
                created_at: 0,
                sort: 0,
            },
            token: Some(secret),
        };
        assert!(!format!("{spec:?}").contains("invented-run-token"));
    }

    #[test]
    fn drop_zeroizes_the_value() {
        struct Probe(std::sync::Arc<std::sync::atomic::AtomicBool>);
        impl Zeroize for Probe {
            fn zeroize(&mut self) {
                self.0.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        }
        let zeroized = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        drop(Secret::new(Probe(zeroized.clone())));
        assert!(zeroized.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test]
    fn redacts_flags_environment_and_json_with_spaces() {
        for text in [
            "cloudflared tunnel run --token invented-token --loglevel info",
            "cloudflared tunnel run --token=invented-token",
            r#"cloudflared --credentials-contents {"AccountTag": "invented-account", "TunnelSecret": "invented-credential"} run"#,
            r#"cloudflared --credentials-contents='{"TunnelSecret": "invented-credential"}' run"#,
            r#"TUNNEL_TOKEN=invented-token TUNNEL_CRED_CONTENTS={"TunnelSecret": "invented-credential"} cloudflared"#,
            r#"TUNNEL_CREDENTIALS_CONTENTS={"TunnelSecret": "invented-credential"} cloudflared"#,
            "https://example.invalid/login?callback=invented-callback&state=ok",
            r#"{"token":"invented-token","apiToken":"invented-api","AccountTag":"invented-account","TunnelSecret":"invented-credential"}"#,
            r#"["tunnel", "--token", "invented-token", "--credentials-contents", "{\"TunnelSecret\": \"invented-credential\"}"]"#,
            r#"{"message":"failed --token invented-token","detail":{"apiToken":"invented-api"}}"#,
            r#"failed: {"token": "invented-token", "bad": "unterminated"#,
            r#"failed: ["--token", "invented-token", "bad"#,
            r#"failed: {\"apiToken\": \"invented-api\", \"bad"#,
            r#"failed --credentials-contents {"TunnelSecret": "invented-credential", malformed tail invented-tail"#,
            "failed --credentials-contents malformed invented-credential tail",
            r#"failed {"apiToken": malformed invented-api tail"#,
            "failed --token 'invented-token malformed tail invented-tail",
            "failed café: --token invented-token",
        ] {
            let redacted = redact_text(text);
            assert!(!redacted.contains("invented-"), "{redacted}");
            assert!(redacted.contains(REDACTED));
        }
        assert_eq!(
            redact_text("cloudflared --token-file token.txt --url localhost:8080"),
            "cloudflared --token-file token.txt --url localhost:8080"
        );
        assert!(redact_text("cloudflared --token invented-token --loglevel info").contains("--loglevel info"));
    }
}
