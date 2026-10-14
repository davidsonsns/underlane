use underlane_core::db::{Db, LogQuery};
use underlane_core::model::{LogLine, Settings, Tunnel, TunnelKind};
use underlane_core::proxy::capture::{BodyCapture, CapturedExchange, ExchangeKind};

const DAY: i64 = 86_400_000;

fn log(tunnel: &str, ts: i64, level: &str, msg: &str) -> LogLine {
    LogLine { id: 0, tunnel_id: tunnel.into(), ts, level: level.into(), message: msg.into(), fields: "{}".into() }
}

fn exchange(id: &str, ts: i64, body: usize) -> CapturedExchange {
    CapturedExchange {
        id: id.into(),
        tunnel_id: "t1".into(),
        rule_index: 0,
        started_at_ms: ts,
        duration_ms: Some(12),
        kind: ExchangeKind::Http,
        method: "POST".into(),
        host: "app.example.com".into(),
        path: format!("/items/{id}"),
        status: Some(201),
        req_headers: vec![("content-type".into(), "application/json".into())],
        resp_headers: vec![],
        req_body: BodyCapture { bytes: vec![b'q'; body], size: body as u64, truncated: false },
        resp_body: BodyCapture { bytes: vec![b'r'; body], size: body as u64, truncated: false },
        error: None,
    }
}

#[test]
fn ttl_sweep_deletes_only_expired_rows() {
    let db = Db::open_in_memory().unwrap();
    let now = 100 * DAY;
    db.insert_logs(&[
        log("t1", now - 8 * DAY, "info", "old"),
        log("t1", now - 6 * DAY, "info", "recent"),
        log("t1", now, "error", "new"),
    ])
    .unwrap();
    db.upsert_request(&exchange("old", now - 7 * DAY - 1, 10)).unwrap();
    db.upsert_request(&exchange("edge", now - 7 * DAY, 10)).unwrap();
    db.upsert_request(&exchange("new", now - 1000, 10)).unwrap();

    let stats = db.sweep(now, 7 * DAY, u64::MAX).unwrap();
    assert_eq!(stats.logs_expired, 1);
    assert_eq!(stats.requests_expired, 1);

    let logs = db.query_logs(&LogQuery { limit: 100, ..Default::default() }).unwrap();
    let msgs: Vec<&str> = logs.iter().map(|l| l.message.as_str()).collect();
    assert_eq!(msgs, vec!["recent", "new"]);
    let ids: Vec<String> = db.list_requests(None, None, 100).unwrap().into_iter().map(|r| r.id).collect();
    assert_eq!(ids, vec!["new".to_string(), "edge".to_string()]);
}

#[test]
fn size_cap_removes_oldest_rows_and_shrinks_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("underlane.db");
    let db = Db::open(&path).unwrap();
    let now = 10 * DAY;
    for i in 0..200 {
        db.upsert_request(&exchange(&format!("r{i:03}"), now - 1000 + i, 50_000)).unwrap();
    }
    let before = db.used_bytes().unwrap();
    assert!(before > 15_000_000, "{before}");

    let cap = 4_000_000;
    let stats = db.sweep(now, 7 * DAY, cap).unwrap();
    assert!(stats.requests_capped > 0);
    assert!(db.used_bytes().unwrap() <= cap);
    let remaining = db.list_requests(None, None, 1000).unwrap();
    // Newest rows survive.
    assert_eq!(remaining.first().unwrap().id, "r199");
    assert!(remaining.iter().all(|r| r.id.as_str() > "r100"));
    // Freed pages are returned (auto_vacuum = INCREMENTAL + WAL checkpoint).
    drop(db);
    let file = std::fs::metadata(&path).unwrap().len();
    assert!(file < before, "file {file} should shrink below {before}");
}

#[test]
fn log_queries_filter_by_tunnel_level_search_and_cursor() {
    let db = Db::open_in_memory().unwrap();
    db.insert_logs(&[
        log("a", 1, "info", "Registered tunnel connection"),
        log("a", 2, "error", "Unable to reach the origin service"),
        log("b", 3, "info", "Registered tunnel connection"),
        log("a", 4, "warn", "Retrying 100% of requests"),
    ])
    .unwrap();
    let q = |q: LogQuery| db.query_logs(&LogQuery { limit: 100, ..q }).unwrap();
    assert_eq!(q(LogQuery { tunnel_id: Some("a".into()), ..Default::default() }).len(), 3);
    let errs = q(LogQuery { levels: vec!["error".into(), "warn".into()], ..Default::default() });
    assert_eq!(errs.iter().map(|l| l.ts).collect::<Vec<_>>(), vec![2, 4]);
    assert_eq!(q(LogQuery { search: Some("origin".into()), ..Default::default() }).len(), 1);
    // LIKE wildcards in the search are literal.
    assert_eq!(q(LogQuery { search: Some("100%".into()), ..Default::default() }).len(), 1);
    assert_eq!(q(LogQuery { search: Some("_".into()), ..Default::default() }).len(), 0);
    let first = q(LogQuery::default());
    let after = q(LogQuery { after_id: Some(first[1].id), ..Default::default() });
    assert_eq!(after.len(), 2);
    let limited = db.query_logs(&LogQuery { limit: 2, ..Default::default() }).unwrap();
    assert_eq!(limited.iter().map(|l| l.ts).collect::<Vec<_>>(), vec![3, 4], "newest page, oldest first");
}

#[test]
fn requests_round_trip_and_upsert() {
    let db = Db::open_in_memory().unwrap();
    let mut x = exchange("x1", 5, 3);
    x.duration_ms = None;
    db.upsert_request(&x).unwrap();
    x.duration_ms = Some(40);
    x.resp_body = BodyCapture { bytes: b"done".to_vec(), size: 4, truncated: true };
    db.upsert_request(&x).unwrap();
    let got = db.get_request("x1").unwrap().unwrap();
    assert_eq!(got, x);
    assert_eq!(db.list_requests(Some("t1"), Some("items"), 10).unwrap().len(), 1);
    assert_eq!(db.list_requests(Some("t1"), Some("201"), 10).unwrap().len(), 1);
    assert_eq!(db.list_requests(Some("nope"), None, 10).unwrap().len(), 0);
}

#[test]
fn request_filters_apply_before_limit_and_combine_categories() {
    let db = Db::open_in_memory().unwrap();
    for (id, ts, status, method, tunnel) in [
        ("get-ok", 1, Some(200), "GET", "t1"),
        ("post-error", 2, Some(502), "POST", "t1"),
        ("get-error", 3, Some(502), "GET", "t1"),
        ("other-tunnel", 4, Some(502), "POST", "t2"),
        ("pending", 5, None, "POST", "t1"),
    ] {
        let mut x = exchange(id, ts, 0);
        x.status = status;
        x.method = method.into();
        x.tunnel_id = tunnel.into();
        db.upsert_request(&x).unwrap();
    }
    // All recent rows are outside the selected status/method; matches must precede LIMIT.
    for i in 0..20 {
        db.upsert_request(&exchange(&format!("recent-{i}"), 100 + i, 0)).unwrap();
    }
    let methods = vec!["GET".into(), "POST".into()];
    let rows = db.list_requests_filtered(Some("t1"), None, &[200, 502], &methods, 2).unwrap();
    assert_eq!(rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), vec!["get-error", "post-error"]);
    let rows = db.list_requests_filtered(Some("t1"), Some("items"), &[200, 502], &["GET".into()], 10).unwrap();
    assert_eq!(rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), vec!["get-error", "get-ok"]);
    assert_eq!(db.list_requests_filtered(Some("t1"), None, &[502], &["POST".into()], 1).unwrap()[0].id, "post-error");
    assert!(db.list_requests_filtered(None, None, &[502], &["POST' OR 1=1 --".into()], 10).unwrap().is_empty());
    assert_eq!(db.list_requests_filtered(None, None, &[], &[], 100).unwrap().len(), db.list_requests(None, None, 100).unwrap().len());
    let posts = db.list_requests_filtered(None, None, &[], &["POST".into()], 100).unwrap();
    assert_eq!(posts.len(), 23);
    assert!(posts.iter().any(|r| r.id == "pending"), "no status selection includes pending responses");
}

#[test]
fn settings_groups_and_tunnels_persist() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("b.db");
    {
        let db = Db::open(&path).unwrap();
        assert_eq!(db.settings().unwrap(), Settings::default());
        let mut s = Settings::default();
        s.theme = "dark".into();
        s.log_ttl_days = 3;
        db.save_settings(&s).unwrap();
        let p = db.ensure_group("Shop").unwrap();
        assert_eq!(db.ensure_group("shop").unwrap().id, p.id, "case-insensitive");
        db.upsert_tunnel(&Tunnel {
            id: "t1".into(),
            group_id: Some(p.id.clone()),
            name: "web".into(),
            kind: TunnelKind::Quick,
            config_path: None,
            origin_url: Some("http://localhost:3000".into()),
            inspect: true,
            autostart: true,
            loglevel: "info".into(),
            created_at: 1,
            sort: 0,
        })
        .unwrap();
    }
    let db = Db::open(&path).unwrap();
    assert_eq!(db.settings().unwrap().theme, "dark");
    let t = db.tunnel("t1").unwrap().unwrap();
    assert!(t.inspect && t.autostart);
    db.delete_tunnel("t1").unwrap();
    assert!(db.groups().unwrap().is_empty(), "empty groups are pruned");
}
