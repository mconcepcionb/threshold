//! Writing a file so nobody ever sees half of it.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Write `text` to `path` atomically: a unique temp file in the same directory,
/// flushed to disk, then renamed over the target. On Windows a rename can fail
/// while a virus scanner or editor briefly holds the target open, so it retries.
pub fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    let tmp = temp_sibling(path);
    let result = (|| {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        drop(file);
        rename_with_retry(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

fn temp_sibling(path: &Path) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let name = format!(
        ".{}.{}.{}.tmp",
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("config"),
        std::process::id(),
        nanos
    );
    path.with_file_name(name)
}

fn rename_with_retry(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut last = None;
    for attempt in 0..6 {
        match std::fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last = Some(e);
                // 32 = sharing violation, 5 = access denied on Windows
                if matches!(
                    last.as_ref().and_then(|e| e.raw_os_error()),
                    Some(32) | Some(5)
                ) {
                    std::thread::sleep(std::time::Duration::from_millis(20 * (attempt + 1)));
                    continue;
                }
                break;
            }
        }
    }
    Err(last.unwrap_or_else(|| std::io::Error::other("rename failed")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_atomic_write_leaves_only_the_target() {
        let dir = std::env::temp_dir().join(format!("threshold-fsutil-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cues.toml");

        write_atomic(&path, "one").unwrap();
        write_atomic(&path, "two").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "two");

        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "temp files were left behind");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
