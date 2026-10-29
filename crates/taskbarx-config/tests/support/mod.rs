use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Each test owns an exclusively created directory, never APPDATA or a shared
/// filename. Keep the resolved root for a containment check before cleanup.
pub struct TestDirectory {
    root: PathBuf,
    path: PathBuf,
}

impl TestDirectory {
    pub fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = fs::canonicalize(std::env::temp_dir()).unwrap();
        loop {
            let path = root.join(format!(
                "taskbarx-config-test-{}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self { root, path },
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("could not create test directory: {error}"),
            }
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        if let Ok(resolved) = fs::canonicalize(&self.path) {
            // Refuse recursive cleanup outside this test's explicitly owned
            // immediate child of the captured temporary root.
            if resolved == self.path && resolved.parent() == Some(self.root.as_path()) {
                let _ = fs::remove_dir_all(&resolved);
            }
        }
    }
}
