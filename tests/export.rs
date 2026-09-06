use std::fs;
use std::io::ErrorKind;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier};
use std::time::{SystemTime, UNIX_EPOCH};

static EXPORT_DIR_SEQ: AtomicU64 = AtomicU64::new(0);

use netboard::{csv_escape, export::save_new};

struct ExportDir(PathBuf);

impl ExportDir {
    fn new() -> Self {
        let pid = std::process::id();
        // pid+nanos can collide when cargo runs these tests in one process.
        let mut seq = EXPORT_DIR_SEQ.fetch_add(1, Ordering::Relaxed);
        loop {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path =
                std::env::temp_dir().join(format!("netboard-integration-{pid}-{nonce}-{seq}"));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                    seq = seq.wrapping_add(1);
                }
                Err(error) => panic!("create export dir: {error}"),
            }
        }
    }
}

impl Drop for ExportDir {
    fn drop(&mut self) {
        // Remove only this test's known file and empty, exclusively created dir.
        let _ = fs::remove_file(self.0.join("export.csv"));
        let _ = fs::remove_dir(&self.0);
    }
}

#[test]
fn escaped_untrusted_text_survives_private_file_export() {
    let dir = ExportDir::new();
    let path = dir.0.join("export.csv");
    let contents = format!(
        "Process,Path\n{},{}\n",
        csv_escape("=1+1"),
        csv_escape("/synthetic/a,\"b\"")
    );
    save_new(&path, &contents).unwrap();
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "Process,Path\n\"'=1+1\",\"/synthetic/a,\"\"b\"\"\"\n"
    );
    assert_eq!(fs::metadata(path).unwrap().permissions().mode() & 0o077, 0);
}

#[test]
fn competing_exports_have_one_winner_and_never_mix_contents() {
    let dir = ExportDir::new();
    let path = dir.0.join("export.csv");
    let barrier = Arc::new(Barrier::new(2));
    let handles: Vec<_> = ["first synthetic export\n", "second synthetic export\n"]
        .into_iter()
        .map(|contents| {
            let path = path.clone();
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                (contents, save_new(&path, contents))
            })
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(
        results.iter().filter(|(_, result)| result.is_ok()).count(),
        1
    );
    for (contents, result) in results {
        match result {
            Ok(()) => assert_eq!(fs::read_to_string(&path).unwrap(), contents),
            Err(error) => assert_eq!(error.kind(), ErrorKind::AlreadyExists),
        }
    }
}
