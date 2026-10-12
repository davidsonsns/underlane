use std::path::PathBuf;

use underlane_core::db::Db;
use underlane_core::import::{import_configs, project_name_for};

#[test]
fn project_names_come_from_the_folder() {
    let home = PathBuf::from(std::env::var("HOME").unwrap());
    assert_eq!(project_name_for(&home.join(".cloudflared/example.yml")), "Personal");
    assert_eq!(project_name_for(&home.join("code/shop/.cloudflared/config.yml")), "shop");
    assert_eq!(project_name_for(&home.join("code/blog/tunnel.yml")), "blog");
}

#[test]
fn imports_files_and_folders_once() {
    let dir = tempfile::tempdir().unwrap();
    let shop = dir.path().join("shop");
    std::fs::create_dir_all(shop.join(".cloudflared")).unwrap();
    std::fs::write(shop.join(".cloudflared/config.yml"), "tunnel: a\ningress:\n  - hostname: a.com\n    service: http://localhost:1\n  - service: http_status:404\n").unwrap();
    std::fs::write(shop.join("compose.yml"), "services: {}\n").unwrap();
    let bad = dir.path().join("bad.yml");
    std::fs::write(&bad, "ingress: [").unwrap();

    let db = Db::open_in_memory().unwrap();
    let r = import_configs(&db, &[shop.clone(), bad.clone()], "info");
    assert_eq!(r.added.len(), 1, "{r:?}");
    assert_eq!(r.failed.len(), 1);
    assert!(r.failed[0].1.contains("not valid YAML"));
    let t = &db.tunnels().unwrap()[0];
    assert_eq!(t.name, "shop");
    assert!(!t.inspect, "inspection is off by default");
    assert_eq!(db.projects().unwrap()[0].name, "shop");

    let again = import_configs(&db, &[shop], "info");
    assert!(again.added.is_empty());
    assert_eq!(again.existing.len(), 1);
}
