//! Temporary directories for the tests: owned by a guard, removed when it is dropped (after a pass and
//! after a failed assertion, which unwinds), named uniquely by the OS-level random suffix so concurrent
//! tests cannot touch one another's data. Set `OMATREE_KEEP_TEST_DIRS=1` to keep them for diagnosis;
//! the path of each kept directory is printed to stderr.

use std::ops::Deref;
use std::path::PathBuf;

pub(crate) struct TestDir {
    path: PathBuf,
    /// `None` after the directory was deliberately kept.
    _guard: Option<tempfile::TempDir>,
}

impl TestDir {
    pub(crate) fn new(tag: &str) -> Self {
        let dir = tempfile::Builder::new()
            .prefix(&format!("omatree-editor-{tag}-"))
            .tempdir()
            .expect("create a temporary test directory");
        let path = dir.path().to_path_buf();
        if std::env::var_os("OMATREE_KEEP_TEST_DIRS").is_some() {
            eprintln!("kept test directory: {}", dir.keep().display());
            return TestDir { path, _guard: None };
        }
        TestDir {
            path,
            _guard: Some(dir),
        }
    }
}

impl AsRef<std::path::Path> for TestDir {
    fn as_ref(&self) -> &std::path::Path {
        &self.path
    }
}

impl From<&TestDir> for PathBuf {
    fn from(d: &TestDir) -> PathBuf {
        d.path.clone()
    }
}

impl Deref for TestDir {
    type Target = PathBuf;
    fn deref(&self) -> &PathBuf {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dropped_guard_removes_the_directory_and_its_contents() {
        let path = {
            let d = TestDir::new("drop");
            std::fs::create_dir_all(d.join("a/b")).unwrap();
            std::fs::write(d.join("a/b/f"), b"x").unwrap();
            d.to_path_buf()
        };
        assert!(!path.exists());
    }

    #[test]
    fn a_failing_test_still_cleans_up() {
        let seen = std::sync::Mutex::new(None);
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let d = TestDir::new("panic");
            *seen.lock().unwrap() = Some(d.to_path_buf());
            panic!("boom");
        }));
        assert!(r.is_err());
        let path = seen.lock().unwrap().take().unwrap();
        assert!(!path.exists(), "the unwinding test left {}", path.display());
    }

    #[test]
    fn concurrent_guards_never_share_a_directory() {
        let a = TestDir::new("same");
        let b = TestDir::new("same");
        assert_ne!(*a, *b);
        std::fs::write(a.join("f"), b"a").unwrap();
        assert!(!b.join("f").exists());
        let (pa, pb) = (a.to_path_buf(), b.to_path_buf());
        drop(a);
        assert!(!pa.exists() && pb.exists(), "dropping one leaves the other");
    }
}
