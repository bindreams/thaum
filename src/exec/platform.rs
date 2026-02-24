//! Platform-specific queries (file ownership, terminal detection).

/// Check whether a file is owned by the current effective user.
pub fn file_owned_by_current_user(path: &str) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        std::fs::metadata(path)
            .map(|m| m.uid() == nix::unistd::geteuid().as_raw())
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        false
    }
}

/// Check whether a file is owned by the current effective group.
pub fn file_owned_by_current_group(path: &str) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        std::fs::metadata(path)
            .map(|m| m.gid() == nix::unistd::getegid().as_raw())
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        false
    }
}
