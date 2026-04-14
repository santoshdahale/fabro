use std::path::{Path, PathBuf};

/// Expand `~/` prefix to the user's home directory.
pub fn expand_tilde(path: &Path) -> PathBuf {
    if let Ok(rest) = path.strip_prefix("~") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    path.to_path_buf()
}

/// Replace the user's home directory prefix with `~` for display.
/// Returns the path unchanged if it is not under the home directory.
pub fn contract_tilde(path: &Path) -> PathBuf {
    if let Some(home) = dirs::home_dir() {
        if let Ok(rest) = path.strip_prefix(&home) {
            return Path::new("~").join(rest);
        }
    }
    path.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_tilde_with_home_prefix() {
        let result = expand_tilde(Path::new("~/foo/bar"));
        assert!(result != Path::new("~/foo/bar"));
        assert!(result.ends_with("foo/bar"));
    }

    #[test]
    fn expand_tilde_without_prefix() {
        assert_eq!(expand_tilde(Path::new("/abs/path")), Path::new("/abs/path"));
    }

    #[test]
    fn contract_tilde_under_home() {
        let home = dirs::home_dir().unwrap();
        let path = home.join("foo/bar");
        assert_eq!(contract_tilde(&path), Path::new("~/foo/bar"));
    }

    #[test]
    fn contract_tilde_outside_home() {
        assert_eq!(
            contract_tilde(Path::new("/tmp/other")),
            Path::new("/tmp/other")
        );
    }
}
