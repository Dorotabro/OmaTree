//! The Linux application identity (ticket E12b): the strings in `packaging/egui/linux/` and the
//! window's own `app_id` must be one and the same, or the desktop cannot match the window to its
//! launcher and icon. The shell checks (`packaging/egui/tests/verify-sources.sh`) cover the same
//! ground from the packaging side; these keep it true for anyone who only runs `cargo test`.

use crate::{window_icon, APP_ID};
use std::fs;
use std::path::PathBuf;

fn packaging(name: &str) -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../packaging/egui/linux")
        .join(name);
    fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

#[test]
fn the_window_app_id_is_the_desktop_files_name_wm_class_icon_and_appstream_id() {
    let desktop = packaging(&format!("{APP_ID}.desktop"));
    assert!(desktop
        .lines()
        .any(|l| l == format!("StartupWMClass={APP_ID}")));
    assert!(desktop.lines().any(|l| l == format!("Icon={APP_ID}")));
    assert!(
        desktop.lines().any(|l| l == "Exec=omatree-egui %f"),
        "one %f, no shell, the egui binary"
    );
    assert!(desktop
        .lines()
        .any(|l| l == "MimeType=application/x-omatree;"));
    assert!(packaging(&format!("{APP_ID}.metainfo.xml")).contains(&format!("<id>{APP_ID}</id>")));
    assert!(packaging(&format!("{APP_ID}.xml")).contains(&format!("<icon name=\"{APP_ID}\"/>")));
    assert!(packaging("stage.sh")
        .lines()
        .any(|l| l == format!("id={APP_ID}")));
}

#[test]
fn the_egui_identity_cannot_collide_with_the_qt_applications() {
    // The Qt package owns io.github.Dorotabro.OmaTree.*; the egui one must share no installed name.
    assert_ne!(APP_ID, "io.github.Dorotabro.OmaTree");
    assert!(APP_ID.starts_with("io.github.Dorotabro.OmaTree"));
    assert!(
        !APP_ID.contains(' ') && APP_ID.matches('.').count() >= 3,
        "a valid reverse-DNS id"
    );
}

#[test]
fn the_mime_registration_claims_only_the_omatree_extension() {
    let xml = packaging(&format!("{APP_ID}.xml"));
    assert!(xml.contains("type=\"application/x-omatree\""));
    assert_eq!(xml.matches("<glob ").count(), 1);
    assert!(xml.contains("glob pattern=\"*.omatree\""));
    assert!(
        !xml.contains("<magic"),
        "a SQLite magic would claim every SQLite file"
    );
}

#[test]
fn the_embedded_window_icon_decodes_at_256_pixels() {
    let icon = window_icon().expect("the embedded PNG decodes");
    assert_eq!((icon.width, icon.height), (256, 256));
    assert_eq!(icon.rgba.len(), 256 * 256 * 4);
    assert!(icon.rgba.chunks(4).any(|px| px[3] > 0), "not blank");
}
