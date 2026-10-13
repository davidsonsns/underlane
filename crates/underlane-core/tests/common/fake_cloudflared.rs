//! Shared CLI and supervisor fixture. Each instance has independent mode and argv files.

use std::path::PathBuf;

pub struct Fake {
    _dir: tempfile::TempDir,
    pub bin: PathBuf,
    pub data: PathBuf,
    pub root: PathBuf,
}

pub fn fake(mode: &str) -> Fake {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let bindir = root.join("bin");
    std::fs::create_dir_all(&bindir).unwrap();
    let bin = bindir.join("cloudflared");
    std::fs::write(&bin, include_str!("fake_cloudflared.py")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    std::fs::write(bindir.join("mode"), mode).unwrap();
    let data = root.join("data");
    Fake {
        _dir: dir,
        bin,
        data,
        root,
    }
}

pub fn argv(f: &Fake) -> (Vec<String>, String) {
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(f.root.join("bin/argv.json")).unwrap())
            .unwrap();
    (
        v["argv"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a.as_str().unwrap().to_string())
            .collect(),
        v["cwd"].as_str().unwrap().to_string(),
    )
}
