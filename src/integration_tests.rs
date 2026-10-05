//! Integration tests: the real application, headless.
//!
//! Each scenario in `tests/qml/` runs the production `main.qml` window in a
//! separate process (Qt's offscreen platform) and drives it with QtTest. The
//! process is this very test executable, started again with the one test
//! `app_child` selected, which runs the application exactly as `main` does,
//! from `tests/qml/runner.qml` through the debug-only `OMATREE_TEST_QML` hook
//! in `src/main.rs`. (A separate integration-test crate under `tests/` cannot
//! link the CXX-Qt objects, which only the binary crate's own targets get.)
//! No production file is changed or replaced.
//!
//! Every run gets its own temporary HOME/XDG directories, so nothing of the
//! user's (notebooks, Omarchy theme, settings) is read or written, and a
//! scenario that hangs is killed. Any output from the process other than the
//! scenario's own `T>` lines (a QML warning, a binding loop, a runtime
//! error) fails the run.
//!
//! Run with `cargo test` (all) or `cargo test integration_tests`.

use std::fs;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// How long a whole scenario may take before it is killed.
const TIMEOUT: Duration = Duration::from_secs(90);

/// A temporary directory removed again on drop, even if the test fails.
struct Sandbox(PathBuf);

impl Sandbox {
    fn new(name: &str) -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("omatree-it-{}-{n}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Sandbox(dir)
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// What a finished scenario printed.
struct Outcome {
    lines: Vec<String>,
    status: String,
}

impl Outcome {
    fn report(&self) -> String {
        self.lines.join("\n")
    }
}

fn json_string(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Runs scenario `name` (`tests/qml/<name>.qml`) and returns its output.
/// `extra` are further `"key": value` JSON members for the scenario.
/// `state` is the XDG_STATE_HOME to use; the default is empty (no Omarchy).
fn run_in(
    sandbox: &Sandbox,
    name: &str,
    extra: &[(&str, String)],
    state: Option<&Path>,
) -> Outcome {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    // The QML is copied beside the parameters, so parallel runs never share
    // a params file and the sources are never written to.
    let qml = sandbox.path("qml");
    fs::create_dir_all(&qml).unwrap();
    for entry in fs::read_dir(manifest.join("tests/qml")).unwrap() {
        let entry = entry.unwrap();
        fs::copy(entry.path(), qml.join(entry.file_name())).unwrap();
    }
    let work = sandbox.path("work");
    fs::create_dir_all(&work).unwrap();
    let mut params = format!(
        "{{\"scenario\": {}, \"dir\": {}",
        json_string(name),
        json_string(&work.to_string_lossy())
    );
    for (key, value) in extra {
        params.push_str(&format!(", {}: {}", json_string(key), value));
    }
    params.push('}');
    fs::write(qml.join("params.json"), params).unwrap();

    let home = sandbox.path("home");
    let default_state = sandbox.path("state");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&default_state).unwrap();

    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "integration_tests::app_child",
            "--nocapture",
            "--test-threads=1",
        ])
        .env_clear()
        .env("OMATREE_APP_CHILD", "1")
        // Qt needs a library path and fonts, and nothing else of ours.
        .envs(std::env::vars().filter(|(k, _)| {
            matches!(
                k.as_str(),
                "PATH" | "LD_LIBRARY_PATH" | "FONTCONFIG_FILE" | "FONTCONFIG_PATH"
            )
        }))
        .env("HOME", &home)
        .env("XDG_STATE_HOME", state.unwrap_or(&default_state))
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("XDG_RUNTIME_DIR", &home)
        .env("LC_ALL", "C.UTF-8")
        .env("QT_QPA_PLATFORM", "offscreen")
        .env("QT_FORCE_STDERR_LOGGING", "1")
        .env("QML_XHR_ALLOW_FILE_READ", "1")
        .env("QML_XHR_ALLOW_FILE_WRITE", "1")
        .env("OMATREE_TEST_QML", qml.join("runner.qml"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("could not start omatree");

    let lines = Arc::new(Mutex::new(Vec::new()));
    let mut readers = Vec::new();
    for mut pipe in [
        Box::new(child.stdout.take().unwrap()) as Box<dyn Read + Send>,
        Box::new(child.stderr.take().unwrap()),
    ] {
        let lines = Arc::clone(&lines);
        readers.push(thread::spawn(move || {
            let mut text = String::new();
            let _ = pipe.read_to_string(&mut text);
            lines
                .lock()
                .unwrap()
                .extend(text.lines().map(str::to_string));
        }));
    }

    let started = Instant::now();
    let status = loop {
        match child.try_wait().unwrap() {
            Some(status) => break status.to_string(),
            None if started.elapsed() > TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                break "killed after the timeout".to_string();
            }
            None => thread::sleep(Duration::from_millis(50)),
        }
    };
    for reader in readers {
        let _ = reader.join();
    }
    let lines = Arc::try_unwrap(lines).unwrap().into_inner().unwrap();
    Outcome { lines, status }
}

/// Asserts that every check passed, the scenario finished, and nothing else
/// (no QML warning, binding loop or error) was printed.
fn assert_clean(outcome: &Outcome) {
    let report = outcome.report();
    assert!(
        outcome.status.contains("exit status: 0") || outcome.status.contains("exit code: 0"),
        "omatree ended abnormally ({}):\n{report}",
        outcome.status
    );
    assert!(
        outcome.lines.iter().any(|l| l.contains("T> DONE fails=0")),
        "scenario did not finish cleanly:\n{report}"
    );
    let passes = outcome
        .lines
        .iter()
        .filter(|l| l.contains("T> PASS"))
        .count();
    assert!(passes > 0, "scenario checked nothing:\n{report}");
    assert!(
        !outcome.lines.iter().any(|l| l.contains("T> FAIL")),
        "failed checks:\n{report}"
    );
    let unexpected: Vec<&String> = outcome
        .lines
        .iter()
        .filter(|l| !l.is_empty() && !l.contains("T> ") && !is_test_harness_line(l))
        .collect();
    assert!(
        unexpected.is_empty(),
        "unexpected output (QML warnings?):\n{}",
        unexpected
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// The child is a test run, so the libtest harness prints a few lines of
/// its own around the application's output.
fn is_test_harness_line(line: &str) -> bool {
    line.starts_with("running ")
        || line.starts_with("test integration_tests::app_child")
        || line.starts_with("test result:")
        // The application's own report of a failed Open, which the
        // replacement scenario provokes on purpose.
        || line.starts_with("omatree: could not open ")
        // The conflict scenario provokes refused saves on purpose.
        || line.starts_with("omatree: save failed: the notebook file was changed by someone else")
}

/// The child process: runs the application when started by `run_in`, and
/// does nothing in an ordinary `cargo test`.
#[test]
fn app_child() {
    if std::env::var_os("OMATREE_APP_CHILD").is_some() {
        crate::run_app();
    }
}

fn scenario(name: &str) {
    let sandbox = Sandbox::new(name);
    let outcome = run_in(&sandbox, name, &[], None);
    assert_clean(&outcome);
}

#[test]
fn note_lifecycle_and_reopen() {
    let sandbox = Sandbox::new("lifecycle");
    let outcome = run_in(&sandbox, "lifecycle", &[], None);
    assert_clean(&outcome);

    // The file on disk, read independently of the application.
    let conn = rusqlite::Connection::open(sandbox.path("work/lifecycle.omatree")).unwrap();
    let mut titles: Vec<String> = conn
        .prepare("SELECT title FROM nodes ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    titles.sort();
    assert_eq!(titles, ["Ideas", "Projects"]);
}

#[test]
fn autosave_writes_the_file() {
    let sandbox = Sandbox::new("autosave");
    let outcome = run_in(&sandbox, "autosave", &[], None);
    assert_clean(&outcome);
    let conn = rusqlite::Connection::open(sandbox.path("work/autosave.omatree")).unwrap();
    let body: String = conn
        .query_row("SELECT body FROM nodes WHERE title = 'Note'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(body, "typed in the editor");
    let checkpoints: i64 = conn
        .query_row("SELECT count(*) FROM checkpoints", [], |r| r.get(0))
        .unwrap();
    assert_eq!(checkpoints, 0, "ordinary editing takes no checkpoint");
}

#[test]
fn new_and_open_replace_the_document() {
    scenario("replacement");
}

#[test]
fn search_reveals_a_deep_note() {
    scenario("search");
}

#[test]
fn moved_expansion_is_saved_and_restored() {
    let sandbox = Sandbox::new("expansion");
    let outcome = run_in(&sandbox, "expansion", &[], None);
    assert_clean(&outcome);
    let conn = rusqlite::Connection::open(sandbox.path("work/expansion.omatree")).unwrap();
    let open: Vec<String> = conn
        .prepare("SELECT n.title FROM expanded_nodes e JOIN nodes n ON n.id = e.node_id ORDER BY n.title")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert!(
        open.contains(&"A1".to_string()) && open.contains(&"B".to_string()),
        "{open:?}"
    );
}

#[test]
fn trash_and_checkpoint_restore() {
    scenario("recovery");
}

#[test]
fn markdown_preview_does_not_change_the_note() {
    scenario("preview");
}

#[test]
fn markdown_preview_loads_no_resources() {
    let sandbox = Sandbox::new("resources");
    // Localhost only: a listener that records every connection made to it.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    let local = sandbox.path("work/local-image.png");
    fs::create_dir_all(sandbox.path("work")).unwrap();
    fs::write(&local, b"\x89PNG not really").unwrap();

    let outcome = run_in(
        &sandbox,
        "resources",
        &[
            ("port", port.to_string()),
            ("local", json_string(&local.to_string_lossy())),
        ],
        None,
    );
    assert_clean(&outcome);

    let mut connections = 0;
    while let Ok((mut stream, _)) = listener.accept() {
        connections += 1;
        let mut buffer = [0u8; 256];
        let _ = stream.read(&mut buffer);
    }
    assert_eq!(connections, 0, "Preview contacted the listener");

    // Control: the listener does work, so zero above means something.
    listener.set_nonblocking(false).unwrap();
    let mut probe = TcpStream::connect(("127.0.0.1", port)).unwrap();
    probe.write_all(b"x").unwrap();
    let _ = probe.shutdown(Shutdown::Both);
    assert!(
        listener.accept().is_ok(),
        "the control connection was not seen"
    );
}

#[test]
fn theme_follows_an_isolated_palette() {
    let sandbox = Sandbox::new("theme");
    // An Omarchy-like state directory of our own: the user's is never read.
    let state = sandbox.path("omarchy-state");
    let theme_dir = state.join("omarchy/current/theme");
    fs::create_dir_all(&theme_dir).unwrap();
    let colors = theme_dir.join("colors.toml");
    fs::write(
        &colors,
        "background = \"#101820\"\nforeground = \"#d0d0d0\"\naccent = \"#ff8800\"\n",
    )
    .unwrap();
    let outcome = run_in(
        &sandbox,
        "theme",
        &[("colors", json_string(&colors.to_string_lossy()))],
        Some(&state),
    );
    assert_clean(&outcome);
}

#[test]
fn keyboard_help_and_dialog_smoke() {
    scenario("keyboard");
}

#[test]
fn moved_expanded_subtree_matches_the_document_when_revealed() {
    scenario("moveexpanded");
}

#[test]
fn markdown_paragraph_and_block_spacing() {
    scenario("spacing");
}

#[test]
fn search_header_is_permanent() {
    scenario("searchheader");
}

#[test]
fn new_notes_start_in_edit_mode() {
    scenario("newnote");
}

#[test]
fn escape_returns_to_the_tree() {
    scenario("escape");
}

#[test]
fn a_pending_title_is_never_lost() {
    scenario("titles");
}

#[test]
fn closing_commits_a_pending_title() {
    let sandbox = Sandbox::new("titleclose");
    let outcome = run_in(&sandbox, "titleclose", &[], None);
    assert_clean(&outcome);
    // Close committed the title and saved it, before the window went away.
    let conn = rusqlite::Connection::open(sandbox.path("work/titleclose.omatree")).unwrap();
    let titles: Vec<String> = conn
        .prepare("SELECT title FROM nodes")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert!(titles.contains(&"Omega".to_string()), "{titles:?}");
}

#[test]
fn an_external_change_blocks_saving_and_autosave() {
    let sandbox = Sandbox::new("conflict");
    let work = sandbox.path("work");
    fs::create_dir_all(&work).unwrap();
    let ready = work.join("ready");
    let done = work.join("done");
    let notebook = work.join("conflict.omatree");
    // The "other instance": waits for the scenario, then commits to the file.
    let other = {
        let (ready, done, notebook) = (ready.clone(), done.clone(), notebook.clone());
        thread::spawn(move || {
            let started = Instant::now();
            while !ready.exists() && started.elapsed() < TIMEOUT {
                thread::sleep(Duration::from_millis(20));
            }
            let conn = rusqlite::Connection::open(&notebook).unwrap();
            conn.execute(
                "UPDATE nodes SET body = 'external' WHERE title = 'Note'",
                [],
            )
            .unwrap();
            drop(conn);
            fs::write(&done, "x").unwrap();
        })
    };
    let outcome = run_in(&sandbox, "conflict", &[], None);
    other.join().unwrap();
    assert_clean(&outcome);

    let conn = rusqlite::Connection::open(&notebook).unwrap();
    let body: String = conn
        .query_row("SELECT body FROM nodes WHERE title = 'Note'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(body, "external", "the external change was not overwritten");
    let copy = rusqlite::Connection::open(work.join("mine.omatree")).unwrap();
    let mine: String = copy
        .query_row("SELECT body FROM nodes WHERE title = 'Note'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(mine, "mine", "Save As kept the in-memory version");
}

#[test]
fn dialogs_have_one_visible_keyboard_focus() {
    scenario("dialogs");
}

#[test]
fn recovery_is_navigable_from_the_keyboard() {
    scenario("recoverykeys");
}
