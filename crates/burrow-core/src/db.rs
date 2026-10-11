//! SQLite storage: app state, cloudflared logs and captured requests, with TTL and size-cap
//! expiry.

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::Serialize;

use crate::model::{LogLine, Project, Settings, Tunnel, TunnelKind};
use crate::proxy::capture::{BodyCapture, CapturedExchange, ExchangeKind};

const SCHEMA_VERSION: i64 = 1;

pub struct Db {
    conn: Mutex<Connection>,
}

#[derive(Debug, Default, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct SweepStats {
    pub logs_expired: usize,
    pub requests_expired: usize,
    pub logs_capped: usize,
    pub requests_capped: usize,
}

/// A row in the request list (no bodies).
#[derive(Debug, Clone, Serialize)]
pub struct RequestSummary {
    pub id: String,
    pub tunnel_id: String,
    pub ts: i64,
    pub duration_ms: Option<i64>,
    pub kind: String,
    pub method: String,
    pub host: String,
    pub path: String,
    pub status: Option<u16>,
    pub resp_size: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct LogQuery {
    pub tunnel_id: Option<String>,
    /// Empty = all levels.
    pub levels: Vec<String>,
    pub search: Option<String>,
    /// Only rows with id < before_id (paging backwards).
    pub before_id: Option<i64>,
    /// Only rows with id > after_id (live tail).
    pub after_id: Option<i64>,
    pub limit: usize,
}

impl Db {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> anyhow::Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> anyhow::Result<Self> {
        // auto_vacuum must be set before the first table exists to take effect.
        conn.execute_batch(
            "PRAGMA auto_vacuum = INCREMENTAL;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;",
        )?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version < 1 {
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
                 CREATE TABLE IF NOT EXISTS projects (
                    id TEXT PRIMARY KEY, name TEXT NOT NULL, sort INTEGER NOT NULL DEFAULT 0);
                 CREATE TABLE IF NOT EXISTS tunnels (
                    id TEXT PRIMARY KEY,
                    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                    name TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    config_path TEXT,
                    origin_url TEXT,
                    inspect INTEGER NOT NULL DEFAULT 0,
                    autostart INTEGER NOT NULL DEFAULT 0,
                    loglevel TEXT NOT NULL DEFAULT 'info',
                    created_at INTEGER NOT NULL,
                    sort INTEGER NOT NULL DEFAULT 0);
                 CREATE TABLE IF NOT EXISTS logs (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    tunnel_id TEXT NOT NULL,
                    ts INTEGER NOT NULL,
                    level TEXT NOT NULL,
                    message TEXT NOT NULL,
                    fields TEXT NOT NULL DEFAULT '{}');
                 CREATE INDEX IF NOT EXISTS logs_tunnel_id ON logs(tunnel_id, id);
                 CREATE INDEX IF NOT EXISTS logs_ts ON logs(ts);
                 CREATE TABLE IF NOT EXISTS requests (
                    id TEXT PRIMARY KEY,
                    tunnel_id TEXT NOT NULL,
                    rule_index INTEGER NOT NULL,
                    ts INTEGER NOT NULL,
                    duration_ms INTEGER,
                    kind TEXT NOT NULL,
                    method TEXT NOT NULL,
                    host TEXT NOT NULL,
                    path TEXT NOT NULL,
                    status INTEGER,
                    req_headers TEXT NOT NULL,
                    resp_headers TEXT NOT NULL,
                    req_body BLOB,
                    req_size INTEGER NOT NULL DEFAULT 0,
                    req_truncated INTEGER NOT NULL DEFAULT 0,
                    resp_body BLOB,
                    resp_size INTEGER NOT NULL DEFAULT 0,
                    resp_truncated INTEGER NOT NULL DEFAULT 0,
                    error TEXT);
                 CREATE INDEX IF NOT EXISTS requests_tunnel_ts ON requests(tunnel_id, ts);
                 CREATE INDEX IF NOT EXISTS requests_ts ON requests(ts);",
            )?;
            conn.execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION};"))?;
        }
        Ok(Db { conn: Mutex::new(conn) })
    }

    fn with<T>(&self, f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> anyhow::Result<T> {
        let conn = self.conn.lock().unwrap();
        Ok(f(&conn)?)
    }

    // ---- settings ----

    pub fn settings(&self) -> anyhow::Result<Settings> {
        let raw: Option<String> = self.with(|c| {
            c.query_row("SELECT value FROM settings WHERE key = 'settings'", [], |r| r.get(0)).optional()
        })?;
        Ok(raw.and_then(|r| serde_json::from_str(&r).ok()).unwrap_or_default())
    }

    pub fn save_settings(&self, s: &Settings) -> anyhow::Result<()> {
        let json = serde_json::to_string(s)?;
        self.with(|c| {
            c.execute(
                "INSERT INTO settings(key, value) VALUES ('settings', ?1)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                [json],
            )
        })?;
        Ok(())
    }

    pub fn kv_get(&self, key: &str) -> anyhow::Result<Option<String>> {
        self.with(|c| c.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0)).optional())
    }

    pub fn kv_set(&self, key: &str, value: &str) -> anyhow::Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO settings(key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                [key, value],
            )
        })?;
        Ok(())
    }

    // ---- projects & tunnels ----

    pub fn projects(&self) -> anyhow::Result<Vec<Project>> {
        self.with(|c| {
            let mut st = c.prepare("SELECT id, name, sort FROM projects ORDER BY sort, name")?;
            let rows = st.query_map([], |r| Ok(Project { id: r.get(0)?, name: r.get(1)?, sort: r.get(2)? }))?;
            rows.collect()
        })
    }

    /// Find a project by name (case-insensitive) or create it.
    pub fn ensure_project(&self, name: &str) -> anyhow::Result<Project> {
        let name = name.trim();
        let name = if name.is_empty() { "Default" } else { name };
        let existing = self.with(|c| {
            c.query_row(
                "SELECT id, name, sort FROM projects WHERE lower(name) = lower(?1)",
                [name],
                |r| Ok(Project { id: r.get(0)?, name: r.get(1)?, sort: r.get(2)? }),
            )
            .optional()
        })?;
        if let Some(p) = existing {
            return Ok(p);
        }
        let p = Project {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            sort: self.with(|c| c.query_row("SELECT COALESCE(MAX(sort), -1) + 1 FROM projects", [], |r| r.get(0)))?,
        };
        self.with(|c| c.execute("INSERT INTO projects(id, name, sort) VALUES (?1, ?2, ?3)", params![p.id, p.name, p.sort]))?;
        Ok(p)
    }

    /// Delete projects that no longer have tunnels.
    pub fn prune_empty_projects(&self) -> anyhow::Result<()> {
        self.with(|c| c.execute("DELETE FROM projects WHERE id NOT IN (SELECT project_id FROM tunnels)", []))?;
        Ok(())
    }

    fn tunnel_from_row(r: &Row) -> rusqlite::Result<Tunnel> {
        let kind: String = r.get(3)?;
        Ok(Tunnel {
            id: r.get(0)?,
            project_id: r.get(1)?,
            name: r.get(2)?,
            kind: TunnelKind::parse(&kind).unwrap_or(TunnelKind::Config),
            config_path: r.get(4)?,
            origin_url: r.get(5)?,
            inspect: r.get(6)?,
            autostart: r.get(7)?,
            loglevel: r.get(8)?,
            created_at: r.get(9)?,
            sort: r.get(10)?,
        })
    }

    const TUNNEL_COLS: &'static str =
        "id, project_id, name, kind, config_path, origin_url, inspect, autostart, loglevel, created_at, sort";

    pub fn tunnels(&self) -> anyhow::Result<Vec<Tunnel>> {
        self.with(|c| {
            let mut st = c.prepare(&format!("SELECT {} FROM tunnels ORDER BY sort, created_at", Self::TUNNEL_COLS))?;
            let rows = st.query_map([], Self::tunnel_from_row)?;
            rows.collect()
        })
    }

    pub fn tunnel(&self, id: &str) -> anyhow::Result<Option<Tunnel>> {
        self.with(|c| {
            c.query_row(&format!("SELECT {} FROM tunnels WHERE id = ?1", Self::TUNNEL_COLS), [id], Self::tunnel_from_row)
                .optional()
        })
    }

    pub fn tunnel_by_config_path(&self, path: &str) -> anyhow::Result<Option<Tunnel>> {
        self.with(|c| {
            c.query_row(
                &format!("SELECT {} FROM tunnels WHERE config_path = ?1", Self::TUNNEL_COLS),
                [path],
                Self::tunnel_from_row,
            )
            .optional()
        })
    }

    pub fn upsert_tunnel(&self, t: &Tunnel) -> anyhow::Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO tunnels(id, project_id, name, kind, config_path, origin_url, inspect, autostart, loglevel, created_at, sort)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                 ON CONFLICT(id) DO UPDATE SET project_id = excluded.project_id, name = excluded.name,
                    kind = excluded.kind, config_path = excluded.config_path, origin_url = excluded.origin_url,
                    inspect = excluded.inspect, autostart = excluded.autostart, loglevel = excluded.loglevel,
                    sort = excluded.sort",
                params![
                    t.id,
                    t.project_id,
                    t.name,
                    t.kind.as_str(),
                    t.config_path,
                    t.origin_url,
                    t.inspect,
                    t.autostart,
                    t.loglevel,
                    t.created_at,
                    t.sort
                ],
            )
        })?;
        Ok(())
    }

    pub fn delete_tunnel(&self, id: &str) -> anyhow::Result<()> {
        self.with(|c| {
            c.execute("DELETE FROM tunnels WHERE id = ?1", [id])?;
            c.execute("DELETE FROM logs WHERE tunnel_id = ?1", [id])?;
            c.execute("DELETE FROM requests WHERE tunnel_id = ?1", [id])
        })?;
        self.prune_empty_projects()
    }

    // ---- logs ----

    /// Insert a batch in one transaction. Returns the assigned ids in order.
    pub fn insert_logs(&self, lines: &[LogLine]) -> anyhow::Result<Vec<i64>> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let mut ids = Vec::with_capacity(lines.len());
        {
            let mut st = tx.prepare_cached("INSERT INTO logs(tunnel_id, ts, level, message, fields) VALUES (?1, ?2, ?3, ?4, ?5)")?;
            for l in lines {
                st.execute(params![l.tunnel_id, l.ts, l.level, l.message, l.fields])?;
                ids.push(tx.last_insert_rowid());
            }
        }
        tx.commit()?;
        Ok(ids)
    }

    /// Newest-last page of logs matching the query.
    pub fn query_logs(&self, q: &LogQuery) -> anyhow::Result<Vec<LogLine>> {
        let mut sql = String::from("SELECT id, tunnel_id, ts, level, message, fields FROM logs WHERE 1=1");
        let mut args: Vec<rusqlite::types::Value> = Vec::new();
        if let Some(t) = &q.tunnel_id {
            sql.push_str(" AND tunnel_id = ?");
            args.push(t.clone().into());
        }
        if !q.levels.is_empty() {
            sql.push_str(&format!(" AND level IN ({})", vec!["?"; q.levels.len()].join(",")));
            args.extend(q.levels.iter().map(|l| rusqlite::types::Value::from(l.clone())));
        }
        if let Some(s) = q.search.as_ref().filter(|s| !s.trim().is_empty()) {
            sql.push_str(" AND (message LIKE ? ESCAPE '\\' OR fields LIKE ? ESCAPE '\\')");
            let pat = format!("%{}%", escape_like(s.trim()));
            args.push(pat.clone().into());
            args.push(pat.into());
        }
        if let Some(b) = q.before_id {
            sql.push_str(" AND id < ?");
            args.push(b.into());
        }
        if let Some(a) = q.after_id {
            sql.push_str(" AND id > ?");
            args.push(a.into());
        }
        sql.push_str(" ORDER BY id DESC LIMIT ?");
        args.push((q.limit.clamp(1, 5000) as i64).into());
        let mut rows: Vec<LogLine> = self.with(|c| {
            let mut st = c.prepare(&sql)?;
            let rows = st.query_map(rusqlite::params_from_iter(args.iter()), |r| {
                Ok(LogLine {
                    id: r.get(0)?,
                    tunnel_id: r.get(1)?,
                    ts: r.get(2)?,
                    level: r.get(3)?,
                    message: r.get(4)?,
                    fields: r.get(5)?,
                })
            })?;
            rows.collect()
        })?;
        rows.reverse();
        Ok(rows)
    }

    pub fn clear_logs(&self, tunnel_id: Option<&str>) -> anyhow::Result<()> {
        self.with(|c| match tunnel_id {
            Some(t) => c.execute("DELETE FROM logs WHERE tunnel_id = ?1", [t]),
            None => c.execute("DELETE FROM logs", []),
        })?;
        Ok(())
    }

    // ---- requests ----

    /// Insert or replace (long-lived exchanges are written when they open and again when they close).
    pub fn upsert_request(&self, x: &CapturedExchange) -> anyhow::Result<()> {
        let req_headers = serde_json::to_string(&x.req_headers)?;
        let resp_headers = serde_json::to_string(&x.resp_headers)?;
        self.with(|c| {
            c.execute(
                "INSERT INTO requests(id, tunnel_id, rule_index, ts, duration_ms, kind, method, host, path, status,
                    req_headers, resp_headers, req_body, req_size, req_truncated, resp_body, resp_size, resp_truncated, error)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)
                 ON CONFLICT(id) DO UPDATE SET duration_ms = excluded.duration_ms, kind = excluded.kind,
                    status = excluded.status, resp_headers = excluded.resp_headers,
                    req_body = excluded.req_body, req_size = excluded.req_size, req_truncated = excluded.req_truncated,
                    resp_body = excluded.resp_body, resp_size = excluded.resp_size,
                    resp_truncated = excluded.resp_truncated, error = excluded.error",
                params![
                    x.id,
                    x.tunnel_id,
                    x.rule_index as i64,
                    x.started_at_ms,
                    x.duration_ms,
                    x.kind.as_str(),
                    x.method,
                    x.host,
                    x.path,
                    x.status,
                    req_headers,
                    resp_headers,
                    (!x.req_body.bytes.is_empty()).then_some(&x.req_body.bytes),
                    x.req_body.size as i64,
                    x.req_body.truncated,
                    (!x.resp_body.bytes.is_empty()).then_some(&x.resp_body.bytes),
                    x.resp_body.size as i64,
                    x.resp_body.truncated,
                    x.error
                ],
            )
        })?;
        Ok(())
    }

    pub fn list_requests(&self, tunnel_id: Option<&str>, search: Option<&str>, limit: usize) -> anyhow::Result<Vec<RequestSummary>> {
        let mut sql = String::from(
            "SELECT id, tunnel_id, ts, duration_ms, kind, method, host, path, status, resp_size, error FROM requests WHERE 1=1",
        );
        let mut args: Vec<rusqlite::types::Value> = Vec::new();
        if let Some(t) = tunnel_id {
            sql.push_str(" AND tunnel_id = ?");
            args.push(t.to_string().into());
        }
        if let Some(s) = search.filter(|s| !s.trim().is_empty()) {
            sql.push_str(" AND (path LIKE ? ESCAPE '\\' OR method LIKE ? ESCAPE '\\' OR host LIKE ? ESCAPE '\\' OR CAST(status AS TEXT) = ?)");
            let pat = format!("%{}%", escape_like(s.trim()));
            args.push(pat.clone().into());
            args.push(pat.clone().into());
            args.push(pat.into());
            args.push(s.trim().to_string().into());
        }
        sql.push_str(" ORDER BY ts DESC LIMIT ?");
        args.push((limit.clamp(1, 5000) as i64).into());
        self.with(|c| {
            let mut st = c.prepare(&sql)?;
            let rows = st.query_map(rusqlite::params_from_iter(args.iter()), |r| {
                Ok(RequestSummary {
                    id: r.get(0)?,
                    tunnel_id: r.get(1)?,
                    ts: r.get(2)?,
                    duration_ms: r.get(3)?,
                    kind: r.get(4)?,
                    method: r.get(5)?,
                    host: r.get(6)?,
                    path: r.get(7)?,
                    status: r.get(8)?,
                    resp_size: r.get::<_, i64>(9)? as u64,
                    error: r.get(10)?,
                })
            })?;
            rows.collect()
        })
    }

    pub fn get_request(&self, id: &str) -> anyhow::Result<Option<CapturedExchange>> {
        self.with(|c| {
            c.query_row(
                "SELECT id, tunnel_id, rule_index, ts, duration_ms, kind, method, host, path, status, req_headers, resp_headers,
                        req_body, req_size, req_truncated, resp_body, resp_size, resp_truncated, error
                 FROM requests WHERE id = ?1",
                [id],
                |r| {
                    let kind: String = r.get(5)?;
                    let req_h: String = r.get(10)?;
                    let resp_h: String = r.get(11)?;
                    Ok(CapturedExchange {
                        id: r.get(0)?,
                        tunnel_id: r.get(1)?,
                        rule_index: r.get::<_, i64>(2)? as usize,
                        started_at_ms: r.get(3)?,
                        duration_ms: r.get(4)?,
                        kind: match kind.as_str() {
                            "websocket" => ExchangeKind::Websocket,
                            "sse" => ExchangeKind::Sse,
                            _ => ExchangeKind::Http,
                        },
                        method: r.get(6)?,
                        host: r.get(7)?,
                        path: r.get(8)?,
                        status: r.get(9)?,
                        req_headers: serde_json::from_str(&req_h).unwrap_or_default(),
                        resp_headers: serde_json::from_str(&resp_h).unwrap_or_default(),
                        req_body: BodyCapture {
                            bytes: r.get::<_, Option<Vec<u8>>>(12)?.unwrap_or_default(),
                            size: r.get::<_, i64>(13)? as u64,
                            truncated: r.get(14)?,
                        },
                        resp_body: BodyCapture {
                            bytes: r.get::<_, Option<Vec<u8>>>(15)?.unwrap_or_default(),
                            size: r.get::<_, i64>(16)? as u64,
                            truncated: r.get(17)?,
                        },
                        error: r.get(18)?,
                    })
                },
            )
            .optional()
        })
    }

    pub fn clear_requests(&self, tunnel_id: Option<&str>) -> anyhow::Result<()> {
        self.with(|c| match tunnel_id {
            Some(t) => c.execute("DELETE FROM requests WHERE tunnel_id = ?1", [t]),
            None => c.execute("DELETE FROM requests", []),
        })?;
        Ok(())
    }

    // ---- expiry ----

    /// Bytes used by live pages (excludes the free list).
    pub fn used_bytes(&self) -> anyhow::Result<u64> {
        self.with(|c| {
            let page_size: i64 = c.query_row("PRAGMA page_size", [], |r| r.get(0))?;
            let pages: i64 = c.query_row("PRAGMA page_count", [], |r| r.get(0))?;
            let free: i64 = c.query_row("PRAGMA freelist_count", [], |r| r.get(0))?;
            Ok(((pages - free).max(0) * page_size) as u64)
        })
    }

    /// Delete logs/requests older than `ttl_ms` (relative to `now_ms`), then delete the oldest
    /// rows until live data fits in `max_bytes`, then return freed pages to the OS.
    pub fn sweep(&self, now_ms: i64, ttl_ms: i64, max_bytes: u64) -> anyhow::Result<SweepStats> {
        let mut stats = SweepStats::default();
        let cutoff = now_ms - ttl_ms;
        // Delete in small batches so the connection lock is never held for long (the UI and
        // the log writer share it).
        loop {
            let n = self.with(|c| c.execute("DELETE FROM logs WHERE id IN (SELECT id FROM logs WHERE ts < ?1 LIMIT 2000)", [cutoff]))?;
            stats.logs_expired += n;
            if n == 0 {
                break;
            }
        }
        loop {
            let n = self.with(|c| c.execute("DELETE FROM requests WHERE id IN (SELECT id FROM requests WHERE ts < ?1 LIMIT 200)", [cutoff]))?;
            stats.requests_expired += n;
            if n == 0 {
                break;
            }
        }

        let mut guard = 0;
        while self.used_bytes()? > max_bytes && guard < 10_000 {
            guard += 1;
            let (oldest_log, oldest_req): (Option<i64>, Option<i64>) = self.with(|c| {
                Ok((
                    c.query_row("SELECT MIN(ts) FROM logs", [], |r| r.get(0))?,
                    c.query_row("SELECT MIN(ts) FROM requests", [], |r| r.get(0))?,
                ))
            })?;
            let n = match (oldest_log, oldest_req) {
                (None, None) => break,
                (Some(l), Some(r)) if r <= l => self.delete_oldest_requests(&mut stats)?,
                (None, Some(_)) => self.delete_oldest_requests(&mut stats)?,
                _ => self.delete_oldest_logs(&mut stats)?,
            };
            if n == 0 {
                break;
            }
        }
        self.with(|c| c.execute_batch("PRAGMA incremental_vacuum; PRAGMA wal_checkpoint(TRUNCATE);"))?;
        Ok(stats)
    }

    fn delete_oldest_logs(&self, stats: &mut SweepStats) -> anyhow::Result<usize> {
        let n = self.with(|c| c.execute("DELETE FROM logs WHERE id IN (SELECT id FROM logs ORDER BY ts LIMIT 200)", []))?;
        stats.logs_capped += n;
        Ok(n)
    }

    fn delete_oldest_requests(&self, stats: &mut SweepStats) -> anyhow::Result<usize> {
        let n = self.with(|c| c.execute("DELETE FROM requests WHERE id IN (SELECT id FROM requests ORDER BY ts LIMIT 10)", []))?;
        stats.requests_capped += n;
        Ok(n)
    }
}

fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}
