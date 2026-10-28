//! egui ships no bold face. Probe a few system fonts at runtime (nothing is bundled, so no
//! licence question); fall back to letter-spacing emphasis when none is found.

use egui::{Context, FontData, FontDefinitions, FontFamily};
use std::sync::Arc;

const CANDIDATES: &[&str] = &[
    "/usr/share/fonts/liberation/LiberationSans-Bold.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
    "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
    "/Library/Fonts/Arial Bold.ttf",
    "C:\\Windows\\Fonts\\arialbd.ttf",
];

pub fn install_bold(ctx: &Context) -> Option<FontFamily> {
    let bytes = CANDIDATES.iter().find_map(|p| std::fs::read(p).ok())?;
    let mut fonts = FontDefinitions::default();
    fonts
        .font_data
        .insert("bold".into(), Arc::new(FontData::from_owned(bytes)));
    let mut stack = vec!["bold".to_owned()];
    // Fall back to the default proportional stack for glyphs the bold face lacks (emoji, ...).
    stack.extend(fonts.families[&FontFamily::Proportional].iter().cloned());
    let family = FontFamily::Name("bold".into());
    fonts.families.insert(family.clone(), stack);
    ctx.set_fonts(fonts);
    Some(family)
}

pub(crate) const EMOJI_FONTS: &[&str] = &[
    "/usr/share/fonts/noto/NotoColorEmoji.ttf",
    "/usr/share/fonts/truetype/noto/NotoColorEmoji.ttf",
    "/usr/share/fonts/google-noto-color-emoji-fonts/Noto-COLR.ttf",
    "/usr/share/fonts/google-noto-emoji/NotoColorEmoji.ttf",
    "/usr/share/fonts/twemoji/twemoji.ttf",
    "/System/Library/Fonts/Apple Color Emoji.ttc",
    "C:\\Windows\\Fonts\\seguiemj.ttf",
];

fn any_exists(paths: &[&str]) -> bool {
    paths.iter().any(|p| std::path::Path::new(p).exists())
}

/// Emoji glyphs come from eframe's system font fallback. This only checks the usual install
/// locations (no subprocess) to give an actionable hint instead of silent tofu boxes.
pub fn emoji_font_hint() -> Option<String> {
    (!any_exists(EMOJI_FONTS)).then(|| {
        "No colour emoji font found in the usual locations; emoji may show as boxes \
         (Linux: install Noto Color Emoji, e.g. `noto-fonts-emoji`). The text itself is unaffected."
            .to_owned()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn any_exists_checks_paths() {
        assert!(!any_exists(&["/definitely/not/here.ttf"]));
        assert!(any_exists(&["/definitely/not/here.ttf", "/"]));
    }
}
