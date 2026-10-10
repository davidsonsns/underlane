use std::collections::HashSet;

use burrow_core::cloudflared::*;
use burrow_core::proxy::capture::{redact_query_in_text, CaptureOptions};

#[test]
fn parses_json_log_lines_and_redacts_debug_headers() {
    let redact = CaptureOptions::default().redaction_set();
    let line = r#"{"connIndex":0,"headers":{"Authorization":["Bearer abc"],"Accept":["*/*"]},"level":"debug","message":"GET https://x.trycloudflare.com/?token=q HTTP/1.1","time":"2026-10-10T06:01:53Z"}"#;
    let p = parse_log_line(line, &redact).unwrap();
    assert_eq!(p.level, "debug");
    assert_eq!(p.ts_ms, Some(1_791_612_113_000));
    assert!(p.fields.contains("[redacted]") && !p.fields.contains("Bearer abc"));
    assert!(p.fields.contains("*/*"));
    assert!(!p.fields.contains("\"message\""));
    let q = CaptureOptions::default().query_redaction_set();
    assert_eq!(redact_query_in_text(&p.message, &q), "GET https://x.trycloudflare.com/?token=[redacted] HTTP/1.1");
}

#[test]
fn redacts_query_secrets_in_structured_fields() {
    let o = CaptureOptions::default();
    let line = r#"{"level":"error","message":"Request failed","dest":"https://x.example.com/a?token=s3cret&x=1","error":"GET /b?password=hunter2 failed","time":"2026-10-10T06:01:53Z"}"#;
    let p = parse_log_line_with(line, &o.redaction_set(), Some(&o.query_redaction_set())).unwrap();
    assert!(!p.fields.contains("s3cret") && !p.fields.contains("hunter2"), "{}", p.fields);
    assert!(p.fields.contains("x=1"));
}

#[test]
fn falls_back_to_plain_text_lines() {
    let p = parse_log_line("2026-10-10T06:01:19Z ERR something broke", &HashSet::new()).unwrap();
    assert_eq!(p.level, "error");
    assert_eq!(p.message, "2026-10-10T06:01:19Z ERR something broke");
    assert!(parse_log_line("   ", &HashSet::new()).is_none());
    let p = parse_log_line("quick tunnel provisioning failed with status 429: error code: 1015", &HashSet::new()).unwrap();
    assert_eq!(p.level, "error");
}

#[test]
fn rfc3339_with_offsets_and_fractions() {
    assert_eq!(parse_rfc3339_ms("1970-01-01T00:00:00Z"), Some(0));
    assert_eq!(parse_rfc3339_ms("1970-01-01T00:00:01.5Z"), Some(1500));
    assert_eq!(parse_rfc3339_ms("1970-01-01T01:00:00+01:00"), Some(0));
    assert_eq!(parse_rfc3339_ms("2000-03-01T00:00:00.123456789Z"), Some(951_868_800_123));
    assert_eq!(parse_rfc3339_ms("garbage"), None);
}

#[test]
fn validation_messages_get_plain_language_hints() {
    let v = interpret_validation(false, "Validating rules from x.yml\nValidation failed: The last ingress rule must match all URLs (i.e. it should not have a hostname or path filter)\n");
    assert!(!v.ok);
    assert!(v.raw.starts_with("The last ingress rule"));
    assert!(v.hint.unwrap().contains("http_status:404"));
    let v = interpret_validation(false, "Validation failed: localhost:4000 is an invalid address, please make sure it has a scheme and a hostname");
    assert!(v.hint.unwrap().contains("http://localhost:3000"));
    let v = interpret_validation(false, "Validation failed: Rule #1 is matching the hostname '', but this will match every hostname, meaning the rules which follow it will never be triggered.");
    assert!(v.hint.unwrap().contains("Rule #1"));
    let v = interpret_validation(true, "Validating rules from x.yml\nOK\n");
    assert!(v.ok && v.hint.is_none());
    assert_eq!(interpret_validation(false, "Validation failed: something new").hint, None);
}

#[test]
fn version_metrics_and_quick_url_parsing() {
    assert_eq!(parse_version("cloudflared version 2026.9.1 (built 2026-09-10T20:52:23Z)"), Some("2026.9.1".into()));
    assert_eq!(metrics_addr_from_message("Starting metrics server on 127.0.0.1:20241/metrics"), Some("127.0.0.1:20241".into()));
    assert_eq!(metrics_addr_from_message("Metrics server stopped"), None);
    assert_eq!(
        quick_url_from_message("|  https://synthetic-quick-3.trycloudflare.com                            |"),
        Some("https://synthetic-quick-3.trycloudflare.com".into())
    );
}

#[test]
fn detects_cloudflared_running_outside_burrow() {
    let ps = "\
  48752 node /Users/me/.cloudflared/example-tunnels.mjs
  48763 cloudflared tunnel --config /Users/me/.cloudflared/example.yml --loglevel debug --output json run
  50001 /opt/homebrew/bin/cloudflared tunnel --config /Users/me/Library/Application Support/com.davidsonsns.burrow/derived/x.yml run
  50002 /opt/homebrew/bin/cloudflared tunnel run 00000000-0000-4000-8000-000000000101
";
    let marker = "/Users/me/Library/Application Support/com.davidsonsns.burrow";
    let id = TunnelIdentity {
        config_path: Some("/Users/me/.cloudflared/example.yml".into()),
        tunnel: Some("00000000-0000-4000-8000-000000000101".into()),
        is_default_config: false,
    };
    let pids: Vec<u32> = external_from_ps(ps, &id, marker).iter().map(|p| p.pid).collect();
    assert_eq!(pids, vec![48763, 50002], "node script and Burrow's own process are ignored");

    // Tunnel names only match whole arguments.
    let named = TunnelIdentity { config_path: Some("/x/api.yml".into()), tunnel: Some("api".into()), is_default_config: false };
    let ps2 = "  1 cloudflared tunnel --config /Users/me/work/api-gw/x.yml run\n  2 cloudflared tunnel run api\n";
    let pids: Vec<u32> = external_from_ps(ps2, &named, marker).iter().map(|p| p.pid).collect();
    assert_eq!(pids, vec![2]);

    // A default config is what a bare `cloudflared tunnel run` (e.g. the launchd service) uses.
    let default = TunnelIdentity { config_path: Some("/Users/me/.cloudflared/config.yml".into()), tunnel: Some("site".into()), is_default_config: true };
    let ps3 = "  7 /opt/homebrew/bin/cloudflared tunnel run\n  8 cloudflared tunnel --url http://localhost:3000\n";
    let pids: Vec<u32> = external_from_ps(ps3, &default, marker).iter().map(|p| p.pid).collect();
    assert_eq!(pids, vec![7]);
}
