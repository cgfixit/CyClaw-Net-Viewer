use std::fs::OpenOptions;
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

/// Create a private export without overwriting files or following symlinks.
/// A collision is reported to the UI; the caller can retry after the next second.
pub fn save_new(path: &Path, contents: &str) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(contents.as_bytes())?;
    file.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::{symlink, PermissionsExt};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn saves_privately_and_refuses_existing_files_and_symlinks() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("netboard-export-{}-{nonce}", std::process::id()));
        // create_dir fails if this path already exists; never reuse another test's data.
        fs::create_dir(&dir).unwrap();
        let path = dir.join("capture.csv");
        save_new(&path, "private endpoints\n").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "private endpoints\n");
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o077, 0);
        assert_eq!(
            save_new(&path, "replacement").unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "private endpoints\n");

        let link = dir.join("link.csv");
        symlink(&path, &link).unwrap();
        assert_eq!(
            save_new(&link, "replacement").unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "private endpoints\n");
        let missing = dir.join("missing.csv");
        let dangling = dir.join("dangling.csv");
        symlink(&missing, &dangling).unwrap();
        assert_eq!(
            save_new(&dangling, "replacement").unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        assert!(!missing.exists());

        fs::remove_file(dangling).unwrap();
        fs::remove_file(link).unwrap();
        fs::remove_file(path).unwrap();
        fs::remove_dir(dir).unwrap();
    }
}
