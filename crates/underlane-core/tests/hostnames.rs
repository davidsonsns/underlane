use std::path::{Path, PathBuf};

use underlane_core::hostnames::{self, HostnameRoute};

const SOURCE: &str = "# Keep this comment on a no-op\ntunnel: example\ncredentials-file: credentials.json\nwarp-routing:\n  enabled: true\noriginRequest:\n  connectTimeout: 10s\ningress:\n  - hostname: app.example.com\n    path: /v1/.*\n    service: http://localhost:3000\n    originRequest:\n      noTLSVerify: true\n    custom: [one, two]\n  - path: /assets/.*\n    service: https://localhost:8443\n  - hostname: app.example.com\n    path: /v2/.*\n    service: ssh://localhost:22\n  - hostname: old.example.com\n    service: hello_world\n  - service: http_status:404\n    originRequest:\n      connectTimeout: 2s\n";

fn fixture(text: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yml");
    std::fs::write(&path, text).unwrap();
    (dir, path)
}

fn added(hostname: &str, service: &str) -> HostnameRoute {
    HostnameRoute {
        index: None,
        hostname: hostname.into(),
        service: service.into(),
        path: None,
    }
}

#[test]
fn loads_only_hostname_rules_with_their_original_indices_and_paths() {
    let (_dir, path) = fixture(SOURCE);
    let snapshot = hostnames::load(&path).unwrap();
    assert_eq!(
        snapshot.routes.iter().map(|r| r.index).collect::<Vec<_>>(),
        vec![Some(0), Some(2), Some(3)]
    );
    assert_eq!(snapshot.routes[0].path.as_deref(), Some("/v1/.*"));
    assert_eq!(snapshot.fallback_service, "http_status:404");
    assert_eq!(snapshot.revision.len(), 16);
    assert!(!snapshot.revision.contains("example"));
}

#[test]
fn adds_updates_and_removes_without_losing_hidden_rules_or_options() {
    let (_dir, path) = fixture(SOURCE);
    let snapshot = hostnames::load(&path).unwrap();
    let mut routes = snapshot.routes[..2].to_vec();
    routes[0].hostname = "new.example.com".into();
    routes[0].service = "localhost:4000".into();
    routes.push(added("*.example.com", "https://localhost:443"));
    let prepared =
        hostnames::prepare(&path, &snapshot.revision, &routes, "http_status:503").unwrap();
    assert!(prepared.changed);
    assert_eq!(prepared.config.source, path);
    assert_eq!(prepared.original, SOURCE);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), SOURCE);
    let root = prepared.config.raw.as_mapping().unwrap();
    let original: serde_norway::Value = serde_norway::from_str(SOURCE).unwrap();
    assert_eq!(root.get("warp-routing"), original.get("warp-routing"));
    assert_eq!(root.get("originRequest"), original.get("originRequest"));
    assert_eq!(
        root.get("credentials-file").unwrap().as_str(),
        Some("credentials.json")
    );
    let rules = root.get("ingress").unwrap().as_sequence().unwrap();
    let old_rules = original.get("ingress").unwrap().as_sequence().unwrap();
    assert_eq!(rules[0].get("custom"), old_rules[0].get("custom"));
    assert_eq!(
        rules[0].get("originRequest"),
        old_rules[0].get("originRequest")
    );
    assert_eq!(rules[0].get("path"), old_rules[0].get("path"));
    assert_eq!(
        rules[0].get("service").unwrap().as_str(),
        Some("http://localhost:4000")
    );
    assert_eq!(rules[1], old_rules[1]);
    assert_eq!(rules[2], old_rules[2]);
    assert_eq!(
        rules[3].get("hostname").unwrap().as_str(),
        Some("*.example.com")
    );
    assert_eq!(
        rules[4].get("originRequest"),
        old_rules[4].get("originRequest")
    );
    assert_eq!(
        rules[4].get("service").unwrap().as_str(),
        Some("http_status:503")
    );
}

#[test]
fn semantic_noop_keeps_original_bytes_including_comments() {
    let (_dir, path) = fixture(SOURCE);
    let snapshot = hostnames::load(&path).unwrap();
    let mut routes = snapshot.routes.clone();
    routes[0].service = "localhost:3000".into();
    let prepared = hostnames::prepare(
        &path,
        &snapshot.revision,
        &routes,
        &snapshot.fallback_service,
    )
    .unwrap();
    assert!(!prepared.changed);
    assert_eq!(prepared.text.as_bytes(), SOURCE.as_bytes());
    assert_eq!(prepared.revision, snapshot.revision);
}

#[test]
fn preserves_special_cloudflared_services() {
    for service in [
        "http_status:404",
        "hello_world",
        "bastion",
        "unix:/tmp/app.sock",
        "unix+tls:/tmp/app.sock",
        "tcp://localhost:22",
        "ssh://localhost:22",
    ] {
        let text = format!("ingress:\n  - hostname: app.example.com\n    service: {service}\n  - service: http_status:404\n");
        let (_dir, path) = fixture(&text);
        let snapshot = hostnames::load(&path).unwrap();
        let prepared = hostnames::prepare(
            &path,
            &snapshot.revision,
            &snapshot.routes,
            &snapshot.fallback_service,
        )
        .unwrap();
        assert!(!prepared.changed, "service {service}");
    }
}

#[test]
fn leaves_unchanged_service_text_for_authoritative_cli_validation() {
    let text = "ingress:\n  - hostname: app.example.com\n    service: '  hello_world  '\n  - service: '  http_status:404  '\n";
    let (_dir, path) = fixture(text);
    let snapshot = hostnames::load(&path).unwrap();
    let prepared = hostnames::prepare(
        &path,
        &snapshot.revision,
        &snapshot.routes,
        &snapshot.fallback_service,
    )
    .unwrap();
    assert!(!prepared.changed);
    assert_eq!(prepared.text, text);
}

#[test]
fn removing_every_hostname_keeps_path_only_rules_and_fallback() {
    let (_dir, path) = fixture(SOURCE);
    let snapshot = hostnames::load(&path).unwrap();
    let prepared =
        hostnames::prepare(&path, &snapshot.revision, &[], &snapshot.fallback_service).unwrap();
    assert_eq!(prepared.config.ingress.len(), 2);
    assert_eq!(
        prepared.config.ingress[0].path.as_deref(),
        Some("/assets/.*")
    );
    assert_eq!(prepared.config.ingress[1].service, "http_status:404");
    assert!(prepared.config.ingress.iter().all(|r| r.hostname.is_none()));
}

#[test]
fn rejects_external_changes_and_revisions_for_a_different_path() {
    let (_dir, path) = fixture(SOURCE);
    let snapshot = hostnames::load(&path).unwrap();
    let other = path.with_file_name("other.yml");
    std::fs::write(&other, SOURCE).unwrap();
    assert!(hostnames::prepare(
        &other,
        &snapshot.revision,
        &snapshot.routes,
        &snapshot.fallback_service
    )
    .unwrap_err()
    .to_string()
    .contains("changed"));
    std::fs::write(&path, format!("{SOURCE}# external change\n")).unwrap();
    assert!(hostnames::prepare(
        &path,
        &snapshot.revision,
        &snapshot.routes,
        &snapshot.fallback_service
    )
    .unwrap_err()
    .to_string()
    .contains("changed"));
}

#[test]
fn rejects_reordering_duplicate_indices_hidden_indices_and_changed_paths() {
    let (_dir, path) = fixture(SOURCE);
    let snapshot = hostnames::load(&path).unwrap();
    let cases = [
        {
            let mut rows = snapshot.routes.clone();
            rows.swap(0, 1);
            rows
        },
        vec![snapshot.routes[0].clone(), snapshot.routes[0].clone()],
        {
            let mut rows = snapshot.routes.clone();
            rows[0].index = Some(1);
            rows
        },
        {
            let mut rows = snapshot.routes.clone();
            rows[0].index = Some(4);
            rows
        },
        {
            let mut rows = snapshot.routes.clone();
            rows[0].index = Some(99);
            rows
        },
        {
            let mut rows = snapshot.routes.clone();
            rows[0].path = Some("/changed".into());
            rows
        },
        {
            let mut row = added("new.example.com", "localhost:3000");
            row.path = Some("/.*".into());
            vec![row]
        },
    ];
    for rows in cases {
        assert!(
            hostnames::prepare(&path, &snapshot.revision, &rows, &snapshot.fallback_service)
                .is_err(),
            "rows {rows:?}"
        );
    }
}

#[test]
fn rejects_duplicate_hostname_and_path_but_accepts_distinct_paths() {
    let (_dir, path) = fixture(SOURCE);
    let snapshot = hostnames::load(&path).unwrap();
    // The two existing app hostnames intentionally have different path filters.
    assert!(hostnames::prepare(
        &path,
        &snapshot.revision,
        &snapshot.routes,
        &snapshot.fallback_service
    )
    .is_ok());
    let mut routes = snapshot.routes.clone();
    routes.push(added("OLD.example.com", "localhost:3000"));
    assert!(hostnames::prepare(
        &path,
        &snapshot.revision,
        &routes,
        &snapshot.fallback_service
    )
    .unwrap_err()
    .to_string()
    .contains("same hostname and path"));
    routes.last_mut().unwrap().hostname = "app.example.com".into();
    assert!(hostnames::prepare(
        &path,
        &snapshot.revision,
        &routes,
        &snapshot.fallback_service
    )
    .is_ok());
}

#[test]
fn existing_duplicates_survive_noops_but_cannot_be_modified_as_duplicates() {
    let text = "ingress:\n  - hostname: app.example.com\n    service: hello_world\n  - hostname: app.example.com\n    service: hello_world\n  - service: http_status:404\n";
    let (_dir, path) = fixture(text);
    let snapshot = hostnames::load(&path).unwrap();
    assert!(
        !hostnames::prepare(
            &path,
            &snapshot.revision,
            &snapshot.routes,
            &snapshot.fallback_service
        )
        .unwrap()
        .changed
    );
    let mut routes = snapshot.routes.clone();
    routes[1].service = "localhost:3000".into();
    assert!(hostnames::prepare(
        &path,
        &snapshot.revision,
        &routes,
        &snapshot.fallback_service
    )
    .is_err());
}

#[test]
fn rejects_invalid_hostname_service_and_empty_fallback() {
    let (_dir, path) = fixture(SOURCE);
    let snapshot = hostnames::load(&path).unwrap();
    for hostname in [
        "",
        " ",
        "https://app.example.com",
        "app.example.com/path",
        "app example.com",
        "app.example.com:443",
        "app\\example.com",
        "app.example.com?query",
        "app.example.com#fragment",
        "user@app.example.com",
        "[app.example.com]",
        "app%2eexample.com",
    ] {
        let rows = vec![added(hostname, "localhost:3000")];
        assert!(
            hostnames::prepare(&path, &snapshot.revision, &rows, &snapshot.fallback_service)
                .is_err(),
            "hostname {hostname:?}"
        );
    }
    for service in ["", " ", "localhost:\n3000", "hello_world\0"] {
        assert!(hostnames::prepare(
            &path,
            &snapshot.revision,
            &[added("app.example.com", service)],
            &snapshot.fallback_service
        )
        .is_err());
        assert!(hostnames::prepare(&path, &snapshot.revision, &snapshot.routes, service).is_err());
    }
}

#[test]
fn rejects_single_origin_malformed_and_missing_or_early_fallback() {
    for text in [
        "url: http://localhost:3000\n",
        "hello-world: true\n",
        "ingress: []\n",
        "ingress:\n  - hostname: app.example.com\n    service: hello_world\n",
        "ingress:\n  - service: hello_world\n  - hostname: app.example.com\n    service: hello_world\n  - service: http_status:404\n",
        "ingress:\n  - hostname: [app.example.com]\n    service: hello_world\n  - service: http_status:404\n",
        "ingress:\n  - hostname: app.example.com\n    path: 42\n    service: hello_world\n  - service: http_status:404\n",
        "ingress:\n  - service: 404\n",
        "ingress:\n  - hostname: ''\n    service: http_status:404\n",
        "ingress:\n  - path: /.*\n    service: http_status:404\n",
    ] {
        let (_dir, path) = fixture(text);
        assert!(hostnames::load(&path).is_err(), "source {text}");
    }
    assert!(hostnames::load(Path::new("/nonexistent/underlane-test-hostnames.yml")).is_err());
}

#[test]
fn can_add_hostname_to_fallback_only_config_and_normalizes_bare_port() {
    let (_dir, path) = fixture("ingress:\n  - service: http_status:404\n");
    let snapshot = hostnames::load(&path).unwrap();
    assert!(snapshot.routes.is_empty());
    let prepared = hostnames::prepare(
        &path,
        &snapshot.revision,
        &[added("app.example.com", "3000")],
        &snapshot.fallback_service,
    )
    .unwrap();
    assert_eq!(prepared.config.ingress[0].service, "http://localhost:3000");
    assert_eq!(prepared.config.ingress[1].service, "http_status:404");
}

#[test]
fn rejects_new_hostnames_fully_shadowed_by_retained_or_new_wildcards() {
    for (earlier, hostname) in [
        ("*.example.com", "api.example.com"),
        ("*.example.com", "Api.example.com"),
        ("*.example.com", "API.EXAMPLE.COM"),
        ("*.example.com", "a.api.example.com"),
        ("*.example.com", "*.api.example.com"),
        ("*", "api.example.com"),
        ("*", "*.example.com"),
    ] {
        let text = format!("ingress:\n  - hostname: '{earlier}'\n    service: http://localhost:3000\n  - service: http_status:404\n");
        let (_dir, path) = fixture(&text);
        let snapshot = hostnames::load(&path).unwrap();
        let mut rows = snapshot.routes.clone();
        rows.push(added(hostname, "localhost:4000"));
        let error =
            hostnames::prepare(&path, &snapshot.revision, &rows, &snapshot.fallback_service)
                .unwrap_err()
                .to_string();
        assert!(error.contains("already matched"), "{error}");
        assert!(error.contains("place this rule before"), "{error}");
        // A newly added broad rule also shadows additions following it.
        rows[0].index = None;
        assert!(
            hostnames::prepare(&path, &snapshot.revision, &rows, &snapshot.fallback_service)
                .is_err()
        );
        // Input position does not change the actual placement after retained rules.
        rows[0] = snapshot.routes[0].clone();
        rows.swap(0, 1);
        assert!(
            hostnames::prepare(&path, &snapshot.revision, &rows, &snapshot.fallback_service)
                .is_err()
        );
    }
}

#[test]
fn accepts_new_hostnames_only_partially_matched_or_outside_existing_wildcard() {
    for (earlier, path_filter, hostname) in [
        ("*.example.com", Some("/v1/.*"), "api.example.com"),
        ("*.example.com", None, "example.com"),
        ("*.example.com", None, "example.net"),
        ("*.example.com", None, "api.otherexample.com"),
        ("*.api.example.com", None, "*.example.com"),
        ("api.example.com", None, "*.example.com"),
        ("*.EXAMPLE.COM", None, "API.EXAMPLE.COM"),
    ] {
        let path_yaml = path_filter
            .map(|path| format!("    path: '{path}'\n"))
            .unwrap_or_default();
        let text = format!("ingress:\n  - hostname: '{earlier}'\n{path_yaml}    service: http://localhost:3000\n  - service: http_status:404\n");
        let (_dir, path) = fixture(&text);
        let snapshot = hostnames::load(&path).unwrap();
        let mut rows = snapshot.routes.clone();
        rows.push(added(hostname, "localhost:4000"));
        assert!(
            hostnames::prepare(&path, &snapshot.revision, &rows, &snapshot.fallback_service)
                .is_ok(),
            "{earlier} -> {hostname}, path {path_filter:?}"
        );
    }
}

#[test]
fn removing_or_changing_broad_rule_allows_a_previously_shadowed_addition() {
    let (_dir, path) = fixture("ingress:\n  - hostname: '*.example.com'\n    service: http://localhost:3000\n  - service: http_status:404\n");
    let snapshot = hostnames::load(&path).unwrap();
    let mut rows = vec![added("api.example.com", "localhost:4000")];
    assert!(
        hostnames::prepare(&path, &snapshot.revision, &rows, &snapshot.fallback_service).is_ok()
    );
    let mut changed = snapshot.routes[0].clone();
    changed.hostname = "*.other.example.com".into();
    rows.insert(0, changed);
    assert!(
        hostnames::prepare(&path, &snapshot.revision, &rows, &snapshot.fallback_service).is_ok()
    );
}

#[test]
fn does_not_rewrite_existing_shadowed_routes() {
    let text = "ingress:\n  - hostname: '*.example.com'\n    service: http://localhost:3000\n  - hostname: api.example.com\n    service: http://localhost:4000\n  - service: http_status:404\n";
    let (_dir, path) = fixture(text);
    let snapshot = hostnames::load(&path).unwrap();
    let prepared = hostnames::prepare(
        &path,
        &snapshot.revision,
        &snapshot.routes,
        &snapshot.fallback_service,
    )
    .unwrap();
    assert!(!prepared.changed);
    assert_eq!(prepared.text, text);
}

#[test]
fn lowercases_new_and_explicitly_changed_hostnames_but_preserves_unchanged_spelling() {
    let text = "ingress:\n  - hostname: Api.Example.COM\n    service: http://localhost:3000\n  - service: http_status:404\n";
    let (_dir, path) = fixture(text);
    let snapshot = hostnames::load(&path).unwrap();
    let unchanged = hostnames::prepare(
        &path,
        &snapshot.revision,
        &snapshot.routes,
        &snapshot.fallback_service,
    )
    .unwrap();
    assert!(!unchanged.changed);
    assert_eq!(unchanged.text, text);

    let mut rows = snapshot.routes.clone();
    rows[0].service = "localhost:4000".into();
    let service_only =
        hostnames::prepare(&path, &snapshot.revision, &rows, &snapshot.fallback_service).unwrap();
    assert_eq!(
        service_only.config.ingress[0].hostname.as_deref(),
        Some("Api.Example.COM")
    );

    rows[0].hostname = "API.example.com".into();
    rows.push(added("NEW.Example.COM", "localhost:5000"));
    let changed =
        hostnames::prepare(&path, &snapshot.revision, &rows, &snapshot.fallback_service).unwrap();
    assert_eq!(
        changed.config.ingress[0].hostname.as_deref(),
        Some("api.example.com")
    );
    assert_eq!(
        changed.config.ingress[1].hostname.as_deref(),
        Some("new.example.com")
    );
}

#[test]
fn rejects_changed_existing_hostname_that_becomes_shadowed() {
    for path_yaml in ["", "    path: /v1/.*\n"] {
        let text = format!("ingress:\n  - hostname: '*.example.com'\n    service: http://localhost:3000\n  - hostname: foo.net\n{path_yaml}    service: http://localhost:4000\n  - service: http_status:404\n");
        let (_dir, path) = fixture(&text);
        let snapshot = hostnames::load(&path).unwrap();
        let mut rows = snapshot.routes.clone();
        rows[1].hostname = "API.EXAMPLE.COM".into();
        let error =
            hostnames::prepare(&path, &snapshot.revision, &rows, &snapshot.fallback_service)
                .unwrap_err()
                .to_string();
        assert!(
            error.contains("`api.example.com` is already matched"),
            "{error}"
        );
    }
}

#[test]
fn rejects_broadened_existing_hostname_that_newly_shadows_later_route() {
    let (_dir, path) = fixture("ingress:\n  - hostname: '*.other.com'\n    service: http://localhost:3000\n  - hostname: api.example.com\n    service: http://localhost:4000\n  - service: http_status:404\n");
    let snapshot = hostnames::load(&path).unwrap();
    let mut rows = snapshot.routes.clone();
    rows[0].hostname = "*.EXAMPLE.COM".into();
    assert!(
        hostnames::prepare(&path, &snapshot.revision, &rows, &snapshot.fallback_service)
            .unwrap_err()
            .to_string()
            .contains("already matched")
    );
}

#[test]
fn allows_service_edits_to_legacy_shadowed_rules_but_rejects_changed_hostnames() {
    let (_dir, path) = fixture("ingress:\n  - hostname: '*.example.com'\n    service: http://localhost:3000\n  - hostname: api.example.com\n    service: http://localhost:4000\n  - service: http_status:404\n");
    let snapshot = hostnames::load(&path).unwrap();
    let mut rows = snapshot.routes.clone();
    rows[1].service = "localhost:5000".into();
    assert!(
        hostnames::prepare(&path, &snapshot.revision, &rows, &snapshot.fallback_service).is_ok()
    );
    rows[1].hostname = "new.example.com".into();
    assert!(
        hostnames::prepare(&path, &snapshot.revision, &rows, &snapshot.fallback_service).is_err()
    );
    // Removing the blocker makes the changed route reachable again.
    rows.remove(0);
    assert!(
        hostnames::prepare(&path, &snapshot.revision, &rows, &snapshot.fallback_service).is_ok()
    );
}
