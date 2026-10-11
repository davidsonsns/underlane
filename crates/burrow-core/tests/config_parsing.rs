use std::path::Path;

use burrow_core::config::{scan_dir, TunnelConfig};

const NAMED: &str = r#"
tunnel: 00000000-0000-4000-8000-000000000101
credentials-file: creds/133c7d8f.json
warp-routing:
  enabled: true
originRequest:
  connectTimeout: 10s
ingress:
  - hostname: app.example.com
    service: http://localhost:4000
  - hostname: api.example.com
    path: /v1/.*
    service: https://localhost:8443
    originRequest:
      noTLSVerify: true
  - hostname: ws.example.com
    service: ws://localhost:9000
  - hostname: ssh.example.com
    service: ssh://localhost:22
  - service: http_status:404
"#;

#[test]
fn parses_named_tunnel_with_ingress() {
    let c = TunnelConfig::parse(Path::new("/projects/shop/.cloudflared/config.yml"), NAMED).unwrap();
    assert_eq!(c.tunnel.as_deref(), Some("00000000-0000-4000-8000-000000000101"));
    // Relative credentials resolve against the config directory.
    assert_eq!(c.credentials_file.as_deref().unwrap(), Path::new("/projects/shop/.cloudflared/creds/133c7d8f.json"));
    assert_eq!(c.ingress.len(), 5);
    assert_eq!(c.ingress[0].hostname.as_deref(), Some("app.example.com"));
    assert_eq!(c.ingress[0].service, "http://localhost:4000");
    assert_eq!(c.ingress[1].path.as_deref(), Some("/v1/.*"));
    assert_eq!(c.ingress[4].hostname, None);
    assert_eq!(c.ingress[4].index, 4);
    assert!(!c.is_single_origin());
    // Project folder name, skipping the generic `.cloudflared/config.yml`.
    assert_eq!(c.display_name(), "shop");
}

#[test]
fn parses_single_origin_url_config() {
    let c = TunnelConfig::parse(Path::new("/x/site.yml"), "tunnel: abc\nurl: http://localhost:8080\n").unwrap();
    assert!(c.is_single_origin());
    assert_eq!(c.ingress.len(), 1);
    assert_eq!(c.ingress[0].service, "http://localhost:8080");
    assert_eq!(c.display_name(), "site");
}

#[test]
fn tilde_paths_expand_to_home() {
    let home = std::env::var("HOME").unwrap();
    let c = TunnelConfig::parse(Path::new("/x/a.yml"), "tunnel: a\ncredentials-file: ~/.cloudflared/a.json\ningress:\n  - service: http_status:404\n").unwrap();
    assert_eq!(c.credentials_file.unwrap(), Path::new(&home).join(".cloudflared/a.json"));
}

#[test]
fn reports_yaml_and_shape_errors() {
    let err = TunnelConfig::parse(Path::new("/x/bad.yml"), "ingress:\n  - service: [unclosed").unwrap_err();
    assert!(err.to_string().contains("not valid YAML"), "{err}");
    let err = TunnelConfig::parse(Path::new("/x/bad.yml"), "ingress:\n  - hostname: a.com\n").unwrap_err();
    assert!(err.to_string().contains("has no service"), "{err}");
    let err = TunnelConfig::parse(Path::new("/x/bad.yml"), "ingress: nope\n").unwrap_err();
    assert!(err.to_string().contains("must be a list"), "{err}");
    let err = TunnelConfig::parse(Path::new("/x/bad.yml"), "- a\n- b\n").unwrap_err();
    assert!(err.to_string().contains("mapping"), "{err}");
}

#[test]
fn empty_file_parses_with_no_rules() {
    let c = TunnelConfig::parse(Path::new("/x/empty.yml"), "").unwrap();
    assert!(c.ingress.is_empty());
}

#[test]
fn scan_finds_tunnel_configs_only() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    std::fs::write(d.join("example.yml"), NAMED).unwrap();
    std::fs::write(d.join("config.yml.bak"), NAMED).unwrap();
    std::fs::write(d.join("docker-compose.yml"), "services:\n  web:\n    image: nginx\n").unwrap();
    std::fs::write(d.join("broken.yaml"), "ingress: [").unwrap();
    std::fs::create_dir(d.join(".cloudflared")).unwrap();
    std::fs::write(d.join(".cloudflared/config.yaml"), "tunnel: x\ningress:\n  - service: http_status:404\n").unwrap();
    let found: Vec<String> = scan_dir(d).iter().map(|p| p.strip_prefix(d).unwrap().to_string_lossy().into_owned()).collect();
    assert_eq!(found, vec![".cloudflared/config.yaml".to_string(), "example.yml".to_string()]);
}
