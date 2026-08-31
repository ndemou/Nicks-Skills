use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static DIRECTORY_COUNTER: AtomicU64 = AtomicU64::new(0);

pub struct TestDir {
    path: PathBuf,
}

impl TestDir {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

pub fn tempdir() -> TestDir {
    let root = std::env::temp_dir();
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after Unix epoch")
        .as_nanos();
    for attempt in 0..1000_u64 {
        let counter = DIRECTORY_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = root.join(format!(
            "tat-test-{}-{seed:x}-{counter:x}-{attempt:x}",
            std::process::id()
        ));
        match fs::create_dir(&path) {
            Ok(()) => return TestDir { path },
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => panic!("create temporary directory '{}': {error}", path.display()),
        }
    }
    panic!("could not create a unique temporary test directory")
}
