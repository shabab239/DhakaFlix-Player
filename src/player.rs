use crate::config::{data_dir, Config, Link, PlayerKind};
use std::path::Path;
use std::process::Command;

fn first_existing(cands: &[&str]) -> Option<String> {
    cands.iter().find(|p| Path::new(p).is_file()).map(|s| s.to_string())
}

/// Look for the player in the usual install spots.
pub fn detect(kind: PlayerKind) -> Option<String> {
    match kind {
        PlayerKind::Vlc => first_existing(&[
            r"C:\Program Files\VideoLAN\VLC\vlc.exe",
            r"C:\Program Files (x86)\VideoLAN\VLC\vlc.exe",
        ]),
        PlayerKind::PotPlayer => first_existing(&[
            r"C:\Program Files\DAUM\PotPlayer\PotPlayerMini64.exe",
            r"C:\Program Files (x86)\DAUM\PotPlayer\PotPlayerMini.exe",
            r"C:\Program Files\PotPlayer\PotPlayerMini64.exe",
            r"C:\Program Files (x86)\PotPlayer\PotPlayerMini.exe",
        ]),
        PlayerKind::Custom => None,
    }
}

pub fn fill_defaults(cfg: &mut Config) {
    if cfg.vlc_path.is_empty() {
        cfg.vlc_path = detect(PlayerKind::Vlc).unwrap_or_default();
    }
    if cfg.pot_path.is_empty() {
        cfg.pot_path = detect(PlayerKind::PotPlayer).unwrap_or_default();
    }
}

fn exe_for<'a>(cfg: &'a Config, kind: PlayerKind) -> &'a str {
    match kind {
        PlayerKind::Vlc => &cfg.vlc_path,
        PlayerKind::PotPlayer => &cfg.pot_path,
        PlayerKind::Custom => &cfg.custom_path,
    }
}

/// Launch `kind` on one URL, or on a generated .m3u8 playlist when there are several.
pub fn launch(cfg: &Config, kind: PlayerKind, items: &[Link]) -> Result<(), String> {
    let exe = exe_for(cfg, kind);
    if exe.is_empty() || !Path::new(exe).is_file() {
        return Err(format!("{} not found - set its path in Settings", kind.label()));
    }
    let target = match items {
        [] => return Err("nothing to play".into()),
        [one] => one.url.clone(),
        many => {
            let dir = data_dir();
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let file = dir.join("playlist.m3u8");
            let mut s = String::from("#EXTM3U\n");
            for l in many {
                s.push_str(&format!("#EXTINF:-1,{}\n{}\n", l.name.replace('\n', " "), l.url));
            }
            std::fs::write(&file, s).map_err(|e| e.to_string())?;
            file.to_string_lossy().into_owned()
        }
    };

    let mut cmd = Command::new(exe);
    match kind {
        PlayerKind::Custom if !cfg.custom_args.trim().is_empty() => {
            let mut used = false;
            for a in cfg.custom_args.split_whitespace() {
                if a.contains("{}") {
                    used = true;
                    cmd.arg(a.replace("{}", &target));
                } else {
                    cmd.arg(a);
                }
            }
            if !used {
                cmd.arg(&target);
            }
        }
        _ => {
            cmd.arg(&target);
        }
    }
    cmd.spawn().map(|_| ()).map_err(|e| format!("failed to start {}: {e}", kind.label()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Launches python as a "custom player" and checks the argument it received.
    /// Needs `DHAKAFLIX_TEST_PYTHON=<path to python.exe>`; run with `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn custom_player_gets_url_and_playlist() {
        let py = std::env::var("DHAKAFLIX_TEST_PYTHON").expect("DHAKAFLIX_TEST_PYTHON");
        let dir = data_dir().join("player_test");
        let _ = std::fs::create_dir_all(&dir);
        let out = dir.join("argv.txt");
        let script = dir.join("echo.py");
        std::fs::write(&script, format!("import sys\nopen(r'{}','w',encoding='utf-8').write(chr(10).join(sys.argv[1:]))\n", out.display())).unwrap();
        let mut cfg = Config::default();
        cfg.custom_path = py;
        cfg.custom_args = format!("{} --x {{}}", script.display());
        let run = |items: &[Link]| {
            let _ = std::fs::remove_file(&out);
            launch(&cfg, PlayerKind::Custom, items).unwrap();
            for _ in 0..50 {
                if let Ok(s) = std::fs::read_to_string(&out) {
                    if !s.is_empty() {
                        return s.replace(char::from(13), "");
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            panic!("player did not run");
        };
        let one = run(&[Link::new("a.mkv", "http://h/a%20b.mkv")]);
        assert_eq!(one, "--x\nhttp://h/a%20b.mkv");
        let many = run(&[Link::new("e1.mkv", "http://h/e1.mkv"), Link::new("e2.mkv", "http://h/e2.mkv")]);
        let pl = many.lines().nth(1).unwrap().to_string();
        assert!(pl.ends_with("playlist.m3u8"), "{pl}");
        let m3u = std::fs::read_to_string(pl).unwrap();
        assert!(m3u.starts_with("#EXTM3U") && m3u.contains("http://h/e1.mkv\n") && m3u.contains("http://h/e2.mkv\n"));
    }
}
