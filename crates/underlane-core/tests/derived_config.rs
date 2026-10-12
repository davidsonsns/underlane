use std::collections::HashMap;
use std::time::Duration;

use underlane_core::config::TunnelConfig;
use underlane_core::derived::{self, derive, plan, preflight_credentials, split_service, write_derived};
use serde_norway::Value;

const CONFIG: &str = r#"tunnel: 00000000-0000-4000-8000-000000000101
credentials-file: creds.json
protocol: quic
originRequest:
  connectTimeout: 5s
  noTLSVerify: true
ingress:
  - hostname: app.example.com
    service: http://localhost:4000
  - hostname: secure.example.com
    service: https://localhost:8443
    originRequest:
      noTLSVerify: false
      originServerName: internal.local
      caPool: certs/ca.pem
      tlsTimeout: 3s
  - hostname: grpc.example.com
    service: https://localhost:50051
    originRequest:
      http2Origin: true
  - hostname: sni.example.com
    service: https://localhost:9443
    originRequest:
      matchSNItoHost: true
  - hostname: sock.example.com
    service: unix:/tmp/app.sock
  - hostname: ssh.example.com
    service: ssh://localhost:22
  - service: http_status:404
"#;

fn load(dir: &std::path::Path) -> TunnelConfig {
    let path = dir.join("tunnel.yml");
    std::fs::write(&path, CONFIG).unwrap();
    TunnelConfig::load(&path).unwrap()
}

#[test]
fn plan_merges_origin_request_and_skips_unsupported_rules() {
    let dir = tempfile::tempdir().unwrap();
    let c = load(dir.path());
    let p = plan(&c);
    let idx: Vec<usize> = p.upstreams.iter().map(|u| u.rule_index).collect();
    assert_eq!(idx, vec![0, 1]);

    let app = &p.upstreams[0];
    assert_eq!((app.scheme.as_str(), app.authority.as_str()), ("http", "localhost:4000"));
    assert!(app.no_tls_verify, "inherits top-level noTLSVerify");
    assert_eq!(app.connect_timeout, Duration::from_secs(5), "inherits top-level connectTimeout");

    let secure = &p.upstreams[1];
    assert!(!secure.no_tls_verify, "rule-level value wins");
    assert_eq!(secure.origin_server_name.as_deref(), Some("internal.local"));
    assert_eq!(secure.ca_pool.as_deref(), Some(dir.path().join("certs/ca.pem").as_path()));
    assert_eq!(secure.tls_timeout, Duration::from_secs(3));

    let skipped: Vec<usize> = p.skipped.iter().map(|(i, _)| *i).collect();
    assert_eq!(skipped, vec![2, 3, 4], "http2Origin, matchSNItoHost and unix are not inspected");
    assert!(p.skipped[0].1.contains("http2Origin"));
}

#[test]
fn derive_rewrites_only_inspected_services_and_keeps_everything_else() {
    let dir = tempfile::tempdir().unwrap();
    let c = load(dir.path());
    let ports: HashMap<usize, u16> = [(0, 41001), (1, 41002)].into_iter().collect();
    let doc = derive(&c, &ports);

    let rules = doc.get("ingress").unwrap().as_sequence().unwrap();
    assert_eq!(rules[0].get("service").unwrap().as_str(), Some("http://127.0.0.1:41001"));
    assert_eq!(rules[1].get("service").unwrap().as_str(), Some("http://127.0.0.1:41002"));
    assert_eq!(rules[2].get("service").unwrap().as_str(), Some("https://localhost:50051"));
    assert_eq!(rules[4].get("service").unwrap().as_str(), Some("unix:/tmp/app.sock"));
    assert_eq!(rules[6].get("service").unwrap().as_str(), Some("http_status:404"));
    // Hostnames and order are untouched.
    assert_eq!(rules[1].get("hostname").unwrap().as_str(), Some("secure.example.com"));
    assert_eq!(rules.len(), 7);
    // Unknown top-level keys survive.
    assert_eq!(doc.get("protocol").unwrap().as_str(), Some("quic"));
    assert_eq!(doc.get("tunnel").unwrap().as_str(), Some("00000000-0000-4000-8000-000000000101"));
    // Relative paths become absolute against the config dir.
    let cred = doc.get("credentials-file").unwrap().as_str().unwrap();
    assert_eq!(cred, dir.path().join("creds.json").to_string_lossy());
    let ca = rules[1].get("originRequest").unwrap().get("caPool").unwrap().as_str().unwrap();
    assert_eq!(ca, dir.path().join("certs/ca.pem").to_string_lossy());
}

#[test]
fn derive_without_ports_is_a_faithful_copy() {
    let dir = tempfile::tempdir().unwrap();
    let c = load(dir.path());
    let doc = derive(&c, &HashMap::new());
    let rules = doc.get("ingress").unwrap().as_sequence().unwrap();
    assert_eq!(rules[0].get("service").unwrap().as_str(), Some("http://localhost:4000"));
}

#[test]
fn original_file_is_never_modified() {
    let dir = tempfile::tempdir().unwrap();
    let c = load(dir.path());
    let before = std::fs::read(dir.path().join("tunnel.yml")).unwrap();
    let mtime = std::fs::metadata(dir.path().join("tunnel.yml")).unwrap().modified().unwrap();
    let doc = derive(&c, &[(0, 1234)].into_iter().collect());
    let out = dir.path().join("data/derived/t.yml");
    write_derived(&doc, &c.source, &out).unwrap();
    assert_eq!(std::fs::read(dir.path().join("tunnel.yml")).unwrap(), before);
    assert_eq!(std::fs::metadata(dir.path().join("tunnel.yml")).unwrap().modified().unwrap(), mtime);

    // The written derived file parses back and points at the proxy.
    let text = std::fs::read_to_string(&out).unwrap();
    assert!(text.starts_with("# Generated by Underlane"));
    let reparsed: Value = serde_norway::from_str(&text).unwrap();
    assert_eq!(reparsed["ingress"][0]["service"].as_str(), Some("http://127.0.0.1:1234"));
}

#[test]
fn single_origin_url_is_rewritten() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("s.yml");
    std::fs::write(&path, "tunnel: x\nurl: https://localhost:3000\n").unwrap();
    let c = TunnelConfig::load(&path).unwrap();
    assert_eq!(plan(&c).upstreams.len(), 1);
    let doc = derive(&c, &[(0, 5555)].into_iter().collect());
    assert_eq!(doc.get("url").unwrap().as_str(), Some("http://127.0.0.1:5555"));
}

#[test]
fn service_urls_split_into_scheme_and_authority() {
    assert_eq!(split_service("http://localhost:3000/"), Some(("http".into(), "localhost:3000".into())));
    assert_eq!(split_service("https://example.com"), Some(("https".into(), "example.com:443".into())));
    assert_eq!(split_service("http://[::1]"), Some(("http".into(), "[::1]:80".into())));
    assert_eq!(split_service("http://[::1]:8080"), Some(("http".into(), "[::1]:8080".into())));
    assert_eq!(split_service("ws://localhost:9000"), Some(("http".into(), "localhost:9000".into())));
    assert_eq!(split_service("wss://localhost"), Some(("https".into(), "localhost:443".into())));
    assert_eq!(split_service("ssh://localhost:22"), None);
    assert_eq!(split_service("http_status:404"), None);
}

#[test]
fn durations_parse_go_style() {
    assert_eq!(derived::parse_duration_str("30s"), Some(Duration::from_secs(30)));
    assert_eq!(derived::parse_duration_str("1m30s"), Some(Duration::from_secs(90)));
    assert_eq!(derived::parse_duration_str("250ms"), Some(Duration::from_millis(250)));
    assert_eq!(derived::parse_duration_str("2h"), Some(Duration::from_secs(7200)));
    assert_eq!(derived::parse_duration_str("45"), Some(Duration::from_secs(45)));
    assert_eq!(derived::parse_duration_str("abc"), None);
}

#[test]
fn credentials_preflight() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.yml");
    let id = "00000000-0000-4000-8000-000000000101";
    std::fs::write(&path, format!("tunnel: {id}\ncredentials-file: c.json\ningress:\n  - service: http_status:404\n")).unwrap();
    let c = TunnelConfig::load(&path).unwrap();
    let err = preflight_credentials(&c).unwrap_err();
    assert!(err.contains("doesn't exist"), "{err}");

    std::fs::write(dir.path().join("c.json"), r#"{"TunnelID":"00000000-0000-0000-0000-000000000000"}"#).unwrap();
    let err = preflight_credentials(&c).unwrap_err();
    assert!(err.contains("belongs to tunnel"), "{err}");

    std::fs::write(dir.path().join("c.json"), format!(r#"{{"TunnelID":"{id}"}}"#)).unwrap();
    assert!(preflight_credentials(&c).is_ok());
}

#[test]
fn single_origin_legacy_tls_flags_are_honoured() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("s.yml");
    std::fs::write(&path, "tunnel: x\nurl: https://localhost:8443\nno-tls-verify: true\norigin-server-name: dev.local\n").unwrap();
    let c = TunnelConfig::load(&path).unwrap();
    let p = plan(&c);
    assert!(p.upstreams[0].no_tls_verify);
    assert_eq!(p.upstreams[0].origin_server_name.as_deref(), Some("dev.local"));
}
