//! `omatree-egui [NOTEBOOK.omatree]`: the egui front end of OmaTree (ticket E4).

use omatree_egui::App;
use std::path::PathBuf;

fn main() -> eframe::Result {
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_inner_size(launch_size())
        .with_title("OmaTree")
        .with_app_id(omatree_egui::APP_ID);
    if let Some(icon) = omatree_egui::window_icon() {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "OmaTree",
        options,
        Box::new(move |cc| Ok(Box::new(App::new(cc, args)))),
    )
}

/// The window's size at launch: 1000x700, or `OMATREE_WINDOW_SIZE=WIDTHxHEIGHT` (a diagnostic for
/// checking small and large windows without a window manager that can resize; nothing else reads
/// it, and a malformed value is ignored).
fn launch_size() -> [f32; 2] {
    std::env::var("OMATREE_WINDOW_SIZE")
        .ok()
        .and_then(|v| {
            let (w, h) = v.split_once('x')?;
            let (w, h) = (w.trim().parse::<f32>().ok()?, h.trim().parse::<f32>().ok()?);
            (w >= 200.0 && h >= 200.0 && w <= 8000.0 && h <= 8000.0).then_some([w, h])
        })
        .unwrap_or([1000.0, 700.0])
}
