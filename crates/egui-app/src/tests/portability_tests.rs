//! Cross-platform defects that can be shown without the other platform (ticket E12c): where the
//! recent-notebooks list lives when `HOME` does not exist, and how Windows' verbatim paths are
//! remembered. Running on Windows or macOS is a separate, unverified matter (docs/PACKAGING_EGUI.md).

use crate::recent::{config_base, without_verbatim_prefix, Platform};
use std::ffi::OsString;
use std::path::PathBuf;

fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
    move |k| {
        pairs
            .iter()
            .find(|(n, _)| *n == k)
            .map(|(_, v)| OsString::from(v))
    }
}

#[test]
fn windows_keeps_its_recent_list_in_appdata_instead_of_silently_dropping_it() {
    // Before E12c the only fallbacks were XDG_CONFIG_HOME and HOME, and Windows has no HOME.
    let vars = [
        ("APPDATA", r"C:\Users\me\AppData\Roaming"),
        ("USERPROFILE", r"C:\Users\me"),
    ];
    assert_eq!(
        config_base(Platform::Windows, env(&vars)),
        Some(PathBuf::from(r"C:\Users\me\AppData\Roaming"))
    );
    assert_eq!(
        config_base(Platform::Other, env(&vars)),
        None,
        "no HOME and no XDG: nothing to write to, as before"
    );
    assert_eq!(config_base(Platform::Windows, env(&[])), None);
    assert_eq!(
        config_base(Platform::Windows, env(&[("APPDATA", "")])),
        None,
        "an empty value counts as unset"
    );
}

#[test]
fn macos_uses_application_support_and_linux_dot_config() {
    let vars = [("HOME", "/Users/me")];
    assert_eq!(
        config_base(Platform::MacOs, env(&vars)),
        Some(PathBuf::from("/Users/me/Library/Application Support"))
    );
    assert_eq!(
        config_base(Platform::Other, env(&[("HOME", "/home/me")])),
        Some(PathBuf::from("/home/me/.config"))
    );
}

#[test]
fn an_explicit_xdg_config_home_wins_on_every_platform() {
    let vars = [
        ("XDG_CONFIG_HOME", "/iso/cfg"),
        ("HOME", "/h"),
        ("APPDATA", "/a"),
    ];
    for p in [Platform::Windows, Platform::MacOs, Platform::Other] {
        assert_eq!(
            config_base(p, env(&vars)),
            Some(PathBuf::from("/iso/cfg")),
            "{p:?}"
        );
    }
    assert_eq!(
        config_base(
            Platform::Other,
            env(&[("XDG_CONFIG_HOME", ""), ("HOME", "/h")])
        ),
        Some(PathBuf::from("/h/.config"))
    );
}

#[test]
fn the_verbatim_prefix_is_removed_only_from_drive_paths() {
    assert_eq!(
        without_verbatim_prefix(r"\\?\C:\Users\me\n.omatree"),
        r"C:\Users\me\n.omatree"
    );
    assert_eq!(without_verbatim_prefix(r"\\?\d:\x"), r"d:\x");
    assert_eq!(
        without_verbatim_prefix(r"\\?\UNC\server\share\n.omatree"),
        r"\\?\UNC\server\share\n.omatree"
    );
    assert_eq!(
        without_verbatim_prefix(r"\\server\share\n.omatree"),
        r"\\server\share\n.omatree"
    );
    assert_eq!(
        without_verbatim_prefix(r"\\?\Volume{1234}\x"),
        r"\\?\Volume{1234}\x"
    );
    assert_eq!(
        without_verbatim_prefix("/home/me/n.omatree"),
        "/home/me/n.omatree"
    );
    assert_eq!(
        without_verbatim_prefix(r"\\?\C"),
        r"\\?\C",
        "too short to be a drive path"
    );
}

// --- start-up failures ------------------------------------------------------------------------------------

use crate::{explain_startup_failure, startup_failure_report};

// The first three are the real messages seen in E12a/E12b (no display; Wayland libraries made
// unreadable under bwrap; DISPLAY set with no X11 libraries in a clean container).
const NO_DISPLAY: &str = r#"WinitEventLoop(Os(OsError { line: 765, file: "/x/winit-0.30.13/src/platform_impl/linux/mod.rs", error: Misc("neither WAYLAND_DISPLAY nor WAYLAND_SOCKET nor DISPLAY is set.") }))"#;
const NO_WAYLAND_LIB: &str = r#"WinitEventLoop(Os(OsError { line: 89, file: "/x/winit-0.30.13/src/platform_impl/linux/wayland/event_loop/mod.rs", error: WaylandError(Connection(NoWaylandLib)) }))"#;
const NO_X11_LIB: &str = r#"WinitEventLoop(Os(OsError { line: 788, file: "/x/winit-0.30.13/src/platform_impl/linux/mod.rs", error: XNotSupported(LibraryOpenError(OpenError { kind: Library, detail: "libX11.so.6: cannot open shared object file" })) }))"#;

#[test]
fn the_three_reproduced_startup_failures_get_a_plain_sentence_and_keep_the_original_text() {
    for (debug, word) in [
        (NO_DISPLAY, "graphical session"),
        (NO_WAYLAND_LIB, "Wayland client library"),
        (NO_X11_LIB, "X11 libraries"),
    ] {
        let why = explain_startup_failure(debug).expect(word);
        assert!(why.contains(word), "{why}");
        let report = startup_failure_report(debug);
        assert!(report.starts_with("omatree-egui: OmaTree could not open its window."));
        assert!(report.contains(why), "the sentence is there");
        assert!(
            report.ends_with(&format!("Details: {debug}")),
            "the original diagnostic is kept whole"
        );
    }
}

#[test]
fn an_unrecognised_failure_is_reported_without_a_guess() {
    let debug = "AppCreation(Custom(\"something else entirely\"))";
    assert_eq!(explain_startup_failure(debug), None);
    let report = startup_failure_report(debug);
    assert_eq!(
        report,
        format!("omatree-egui: OmaTree could not open its window.\nDetails: {debug}")
    );
}

// --- native dialogs -------------------------------------------------------------------------------------------

use crate::files::{native_picker, Launcher, PickKind, PickRequest};
use std::sync::mpsc::TryRecvError;

#[test]
fn the_native_picker_only_queues_until_the_window_launches_it() {
    let ctx = eframe::egui::Context::default();
    let (mut picker, launcher) = native_picker();
    assert_eq!(launcher.queued(), 0);
    let req = PickRequest {
        kind: PickKind::Open,
        dir: None,
        name: String::new(),
    };
    let answer = picker(&req, &ctx);
    // Asking returns at once with a channel that has no answer: nothing was shown, nothing was cancelled.
    assert_eq!(launcher.queued(), 1);
    assert!(matches!(answer.try_recv(), Err(TryRecvError::Empty)));
    // Launching with nothing queued does nothing, and with the typed fallback there is nothing to launch.
    Launcher::default().launch(None);
    assert_eq!(Launcher::default().queued(), 0);
}

// --- local time in Recovery ----------------------------------------------------------------------------------------

use crate::recovery_ui::{format_at_offset, format_local, format_utc};

#[test]
fn a_stored_utc_instant_is_written_at_any_offset_with_the_offset_named() {
    assert_eq!(
        format_at_offset(1_700_000_000, 0),
        "2023-11-14 22:13 +00:00"
    );
    assert_eq!(
        format_at_offset(1_700_000_000, 3600),
        "2023-11-14 23:13 +01:00"
    );
    assert_eq!(
        format_at_offset(1_700_000_000, -3 * 3600 - 1800),
        "2023-11-14 18:43 -03:30"
    );
    assert_eq!(
        format_at_offset(1_700_000_000, 5 * 3600 + 2700),
        "2023-11-15 03:58 +05:45",
        "the date follows the offset"
    );
    assert_eq!(
        format_at_offset(0, -3600),
        "1969-12-31 23:00 -01:00",
        "before 1970 too"
    );
    // The UTC form, used when no zone is available, is unchanged.
    assert_eq!(format_utc(1_700_000_000), "2023-11-14 22:13 UTC");
    // And whatever the machine's zone is, the local form is the same instant: its offset is a real offset.
    let local = format_local(1_700_000_000);
    assert!(
        local.len() == 23 && (local.as_bytes()[17] == b'+' || local.as_bytes()[17] == b'-'),
        "{local}"
    );
}

/// Run by the next test in a child process with `TZ` set (changing `TZ` inside the test process would race
/// the other tests): prints the local form of four instants around the 2024 daylight-saving changes.
#[test]
#[ignore = "child process of local_time_follows_daylight_saving"]
fn local_time_probe() {
    for secs in [
        1_711_846_799_i64,
        1_711_846_800,
        1_729_990_799,
        1_729_990_800,
    ] {
        println!("PROBE {secs} {}", format_local(secs));
    }
}

#[cfg(unix)]
#[test]
fn local_time_follows_daylight_saving_on_both_sides_of_a_change() {
    // 2024-03-31 00:59:59 UTC and 01:00:00 UTC (Prague springs forward); 2024-10-27 00:59:59 UTC and
    // 01:00:00 UTC (Prague falls back). Needs the system's zone database, which is not part of the test.
    if !std::path::Path::new("/usr/share/zoneinfo/Europe/Prague").exists() {
        eprintln!("skipped: no /usr/share/zoneinfo");
        return;
    }
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "tests::portability_tests::local_time_probe",
            "--nocapture",
        ])
        .env("TZ", "Europe/Prague")
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = text.lines().filter(|l| l.starts_with("PROBE")).collect();
    assert_eq!(
        lines,
        [
            "PROBE 1711846799 2024-03-31 01:59 +01:00",
            "PROBE 1711846800 2024-03-31 03:00 +02:00",
            "PROBE 1729990799 2024-10-27 02:59 +02:00",
            "PROBE 1729990800 2024-10-27 02:00 +01:00",
        ],
        "{text}"
    );
}

// ---- test hygiene (E13a): the temporary directories of the tests clean up after themselves ----

#[test]
fn a_dropped_test_dir_removes_the_directory_and_its_contents() {
    let path = {
        let d = super::TestDir::new("drop");
        std::fs::create_dir_all(d.join("a/b")).unwrap();
        std::fs::write(d.join("a/b/f"), b"x").unwrap();
        d.to_path_buf()
    };
    assert!(!path.exists());
}

#[test]
fn a_failing_test_still_cleans_up_its_directory() {
    let seen = std::sync::Mutex::new(None);
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let d = super::TestDir::new("panic");
        *seen.lock().unwrap() = Some(d.to_path_buf());
        panic!("boom");
    }));
    assert!(r.is_err());
    let path = seen.lock().unwrap().take().unwrap();
    assert!(!path.exists(), "the unwinding test left {}", path.display());
}

#[test]
fn test_dirs_with_the_same_tag_never_share_a_directory() {
    let a = super::TestDir::new("same");
    let b = super::TestDir::new("same");
    assert_ne!(*a, *b);
    std::fs::write(a.join("f"), b"a").unwrap();
    assert!(!b.join("f").exists());
    let (pa, pb) = (a.to_path_buf(), b.to_path_buf());
    drop(a);
    assert!(!pa.exists() && pb.exists(), "dropping one leaves the other");
}
