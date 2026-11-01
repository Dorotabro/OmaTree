//! Plain-language context for start-up failures (ticket E12c).
//!
//! When no window can be opened, eframe returns an error whose text names toolkit internals
//! (`WinitEventLoop(Os(OsError { line: 765, file: ".../winit-0.30.13/...", ... }))`). The exit status
//! stays non-zero and the original text is still printed in full; this only adds one sentence in front
//! of it saying what is probably missing, for the failures that were reproduced (no display
//! variable, a missing Wayland or X11 library; a GL failure was not reproduced, so it gets none). An error that
//! matches none of them gets no sentence: guessing would be misleading.

/// A sentence about the likely cause of a start-up failure whose `Debug` text is `debug`, if it is
/// one of the recognised kinds.
pub fn explain(debug: &str) -> Option<&'static str> {
    const RULES: &[(&[&str], &str)] = &[
        (
            &["neither WAYLAND_DISPLAY nor WAYLAND_SOCKET nor DISPLAY is set"],
            "There is no graphical session to open a window in (neither WAYLAND_DISPLAY nor DISPLAY is set). Start OmaTree from a desktop session, not from a plain terminal login or over ssh without display forwarding.",
        ),
        (
            &["NoWaylandLib", "NoWaylandSupport"],
            "A Wayland client library (libwayland-client, libwayland-egl or libxkbcommon) could not be loaded. Install your distribution's Wayland client libraries, or start OmaTree under X11 by unsetting WAYLAND_DISPLAY.",
        ),
        (
            &["XNotSupported", "LibraryOpenError", "XOpenDisplayFailed"],
            "The X11 libraries (libX11, libxcb, libXcursor, libXi, libxkbcommon-x11) could not be loaded, or the X server could not be reached. Install them, or check that DISPLAY names a running X server.",
        ),
    ];
    RULES
        .iter()
        .find(|(needles, _)| needles.iter().any(|n| debug.contains(n)))
        .map(|(_, sentence)| *sentence)
}

/// The text printed to standard error for a start-up failure: the explanation (when there is one)
/// and always the original diagnostic.
pub fn report(debug: &str) -> String {
    match explain(debug) {
        Some(why) => {
            format!("omatree-egui: OmaTree could not open its window.\n{why}\nDetails: {debug}")
        }
        None => format!("omatree-egui: OmaTree could not open its window.\nDetails: {debug}"),
    }
}
