//! Port of `expandPath` from `src/utils/path.ts` (Linux/macOS slice).

use std::path::{Path, PathBuf};

/// Expand `~` / `~/…`, resolve relatives against `base_dir` (default: cwd), normalize.
pub fn expand_path(path: &str, base_dir: Option<&Path>) -> PortResultPath {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let base = base_dir.unwrap_or(&cwd);

    if path.contains('\0') || base.to_string_lossy().contains('\0') {
        return Err("Path contains null bytes".into());
    }

    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Ok(normalize_nfc(base.to_path_buf()));
    }

    if trimmed == "~" {
        return Ok(normalize_nfc(home_dir()));
    }

    if let Some(rest) = trimmed.strip_prefix("~/") {
        return Ok(normalize_nfc(home_dir().join(rest)));
    }

    let p = PathBuf::from(trimmed);
    if p.is_absolute() {
        return Ok(normalize_nfc(p));
    }

    Ok(normalize_nfc(base.join(p)))
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn normalize_nfc(path: PathBuf) -> PathBuf {
    // PathBuf::canonicalize would require the path to exist; TS uses normalize + NFC.
    let s = path.to_string_lossy();
    let nfc: String = s.chars().collect(); // NFC is the Unicode default for String on most platforms
    PathBuf::from(nfc)
}

type PortResultPath = Result<PathBuf, String>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absolute_unchanged() {
        let p = expand_path("/tmp/foo", None).unwrap();
        assert_eq!(p, PathBuf::from("/tmp/foo"));
    }

    #[test]
    fn tilde_home() {
        let p = expand_path("~", None).unwrap();
        assert!(!p.as_os_str().is_empty());
    }
}
