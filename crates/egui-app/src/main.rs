//! `omatree-egui [NOTEBOOK.omatree]`: the egui front end of OmaTree (ticket E4).

use omatree_egui::App;
use std::path::PathBuf;

fn main() -> eframe::Result {
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 700.0])
            .with_title("OmaTree")
            .with_app_id("omatree-egui"),
        ..Default::default()
    };
    eframe::run_native(
        "OmaTree",
        options,
        Box::new(move |cc| Ok(Box::new(App::new(cc, args)))),
    )
}
