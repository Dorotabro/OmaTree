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
    ctx.set_fonts(bold_definitions(bytes));
    Some(FontFamily::Name("bold".into()))
}

pub(crate) fn bold_definitions(bytes: Vec<u8>) -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    fonts
        .font_data
        .insert("bold".into(), Arc::new(FontData::from_owned(bytes)));
    let mut stack = vec!["bold".to_owned()];
    // Fall back to the default proportional stack for glyphs the bold face lacks (emoji, ...).
    stack.extend(fonts.families[&FontFamily::Proportional].iter().cloned());
    let family = FontFamily::Name("bold".into());
    fonts.families.insert(family, stack);
    fonts
}

/// How much lower, as a fraction of the font size, the bold face's text has to sit on a line to
/// share a baseline with the default face (negative: higher).
///
/// egui hangs every glyph from the top of its line box using *that face's own* ascent, so two
/// faces with different vertical metrics land on different baselines when they share a line.
/// With one line height for both, a glyph's baseline is `ascent - face_height / 2 + line_height / 2`
/// below the line's top (`Glyph::pos`, before egui rounds it to a pixel), so the difference of
/// `ascent - face_height / 2` between the two faces is exactly the shift needed. It is read from
/// the faces egui actually uses (a large probe size makes egui's own 1/32-point rounding of the
/// metrics negligible), so it follows whatever bold face was found, and the layout applies it to
/// the line box *before* egui snaps the baseline to a pixel; a fixed offset cannot do that, which
/// is why the glyphs of two faces were a pixel apart at some sizes and scales and not at others.
pub fn bold_drop(ctx: &Context, bold: &FontFamily) -> f32 {
    const PROBE: f32 = 64.0;
    let from_top = |family: &FontFamily| {
        let mut job = egui::text::LayoutJob::default();
        job.append(
            "H",
            0.0,
            egui::TextFormat::simple(
                egui::FontId::new(PROBE, family.clone()),
                egui::Color32::WHITE,
            ),
        );
        let galley = ctx.fonts_mut(|f| f.layout_job(job));
        let g = &galley.rows[0].glyphs[0];
        g.font_face_ascent - 0.5 * g.font_face_height
    };
    (from_top(&FontFamily::Proportional) - from_top(bold)) / PROBE
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

    /// The font atlas as the application's renderer would hold it, built from the texture deltas.
    #[derive(Default)]
    struct Atlas {
        size: [usize; 2],
        alpha: Vec<u8>,
    }

    impl Atlas {
        fn apply(&mut self, delta: &egui::epaint::textures::TexturesDelta) {
            for (id, ds) in &delta.set {
                if *id != egui::TextureId::default() {
                    continue;
                }
                for d in ds {
                    let egui::ImageData::Color(img) = &d.image;
                    let [w, h] = img.size;
                    match d.pos {
                        None => {
                            self.size = [w, h];
                            self.alpha = img.pixels.iter().map(|p| p.a()).collect();
                        }
                        Some([x0, y0]) => {
                            for y in 0..h {
                                for x in 0..w {
                                    self.alpha[(y0 + y) * self.size[0] + x0 + x] =
                                        img.pixels[y * w + x].a();
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// The ink bottom (physical pixels) of every painted glyph of `job`, with its character: the
    /// lowest atlas row that has ink in the glyph's quad, found in the pixels the renderer would
    /// draw. The bottom of a flat-bottomed letter is its baseline.
    fn ink_bottoms(
        ctx: &Context,
        atlas: &mut Atlas,
        job: egui::text::LayoutJob,
    ) -> Vec<(char, f32)> {
        let mut out = Vec::new();
        for _ in 0..2 {
            let mut chars = Vec::new();
            let mut o = ctx.run_ui(Default::default(), |ui| {
                let g = ui.ctx().fonts_mut(|f| f.layout_job(job.clone()));
                chars = g
                    .rows
                    .iter()
                    .flat_map(|r| r.glyphs.iter())
                    .filter(|gl| !gl.uv_rect.is_nothing())
                    .map(|gl| gl.chr)
                    .collect::<Vec<_>>();
                ui.painter()
                    .galley(egui::pos2(10.0, 10.0), g, egui::Color32::WHITE);
            });
            atlas.apply(&o.textures_delta);
            o.textures_delta.clear();
            let ppp = o.pixels_per_point;
            let mut quads = Vec::new();
            for p in ctx.tessellate(std::mem::take(&mut o.shapes), ppp) {
                let egui::epaint::Primitive::Mesh(m) = p.primitive else {
                    continue;
                };
                for quad in m.vertices.chunks(4) {
                    let lo = |f: fn(&egui::epaint::Vertex) -> f32| {
                        quad.iter().map(f).fold(f32::MAX, f32::min)
                    };
                    let hi = |f: fn(&egui::epaint::Vertex) -> f32| {
                        quad.iter().map(f).fold(f32::MIN, f32::max)
                    };
                    let [w, h] = atlas.size;
                    let px = |v: f32, n: usize| (v * n as f32).round() as usize;
                    let (x0, x1) = (px(lo(|v| v.uv.x), w), px(hi(|v| v.uv.x), w));
                    let (y0, y1) = (px(lo(|v| v.uv.y), h), px(hi(|v| v.uv.y), h));
                    if x0 < 2 && y0 < 2 {
                        continue; // a solid rectangle (background, underline): the atlas's white pixel
                    }
                    let last = (y0..y1)
                        .rev()
                        .find(|y| (x0..x1).any(|x| atlas.alpha[y * w + x] >= 128));
                    quads.push(last.map(|l| lo(|v| v.pos.y) * ppp + (l + 1 - y0) as f32));
                }
            }
            assert_eq!(quads.len(), chars.len(), "one quad per painted glyph");
            out = chars
                .into_iter()
                .zip(quads)
                .filter_map(|(c, q)| q.map(|q| (c, q)))
                .collect();
        }
        out
    }

    /// Lay `src` out with the editor's own `build_job` (the application's path: its fonts, its
    /// bold family, its line boxes), paint it, and return the baseline of each flat-bottomed
    /// letter in it, in physical pixels. `corrected` false leaves out the bold face's correction.
    fn baselines_of(src: &str, size: f32, ppp: f32, corrected: bool) -> Vec<(char, f32)> {
        use crate::{layout, markdown};
        let ctx = Context::default();
        ctx.set_pixels_per_point(ppp);
        let bytes = CANDIDATES
            .iter()
            .find_map(|p| std::fs::read(p).ok())
            .unwrap();
        ctx.set_fonts(bold_definitions(bytes));
        let family = FontFamily::Name("bold".into());
        let mut atlas = Atlas::default();
        let mut first = ctx.run_ui(Default::default(), |_| {});
        atlas.apply(&first.textures_delta);
        first.textures_delta.clear();
        let drop = if corrected {
            bold_drop(&ctx, &family)
        } else {
            0.0
        };
        let spans = markdown::parse(src);
        let img = |_: usize, _: &str, _: Option<u16>| None;
        let params = layout::Params {
            base_size: size,
            text_color: egui::Color32::WHITE,
            heading_colors: [egui::Color32::WHITE; 6],
            quote_color: egui::Color32::WHITE,
            code_color: egui::Color32::WHITE,
            dim_color: egui::Color32::GRAY,
            accent: egui::Color32::BLUE,
            code_bg: egui::Color32::DARK_GRAY,
            bold_family: Some(family),
            bold_drop: drop,
            pixels_per_point: ppp,
            selection: src.len()..src.len(),
            image_height: &img,
            table_height: &|_| 90.0,
        };
        let (job, _) = layout::build_job(src, &spans, &params, 100_000.0);
        ink_bottoms(&ctx, &mut atlas, job)
            .into_iter()
            .filter(|(c, _)| "HEŽ".contains(*c))
            .collect()
    }

    const MIXED: &[&str] = &[
        // regular, bold, italic and bold-italic on one line
        "H **H** *H* ***H*** H\n",
        // a block quote, a link, Czech letters, an emoji, a list marker, a task
        "> H **H** *H*\n",
        "[H](https://example.org) **H** H\n",
        "Ž **Ž** E **E** ***Ž*** E\n",
        "H \u{1F600} **H** \u{1F600} H\n",
        "- H **H**\n",
        "# H **H** *H* H\n",
        "## H **H** H\n",
    ];

    /// Every flat-bottomed letter of a line, whatever its style, ends on the same pixel row.
    fn aligned(src: &str, size: f32, ppp: f32, corrected: bool) -> Result<(), String> {
        let found = baselines_of(src, size, ppp, corrected);
        let first = found.first().ok_or("nothing painted")?.1;
        match found.iter().find(|(_, y)| (y - first).abs() > 0.01) {
            None => Ok(()),
            Some((c, y)) => Err(format!(
                "{src:?} at {size}pt, {ppp}x: {c:?} ends at {y}, the first letter at {first} ({found:?})"
            )),
        }
    }

    #[test]
    fn regular_bold_italic_and_bold_italic_letters_share_a_baseline() {
        if !CANDIDATES.iter().any(|p| std::path::Path::new(p).exists()) {
            return; // no bold face on this machine: the host falls back to letter-spacing
        }
        let mut failures = Vec::new();
        for ppp in [1.0, 1.25, 1.5, 2.0] {
            for size in [12.0, 13.5, 15.0, 17.0, 20.0, 27.0] {
                for src in MIXED {
                    if let Err(e) = aligned(src, size, ppp, true) {
                        failures.push(e);
                    }
                }
            }
        }
        assert!(
            failures.is_empty(),
            "{} misaligned:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }

    /// The measurement above can see the defect: without the correction the same lines are
    /// misaligned at some size and scale (this is what the application showed before U1).
    #[test]
    fn the_measurement_sees_a_missing_correction() {
        if !CANDIDATES.iter().any(|p| std::path::Path::new(p).exists()) {
            return;
        }
        let misaligned = [1.0, 1.25, 1.5, 2.0]
            .into_iter()
            .flat_map(|ppp| [13.5, 15.0, 20.0].map(|size| (ppp, size)))
            .filter(|&(ppp, size)| aligned(MIXED[0], size, ppp, false).is_err())
            .count();
        assert!(
            misaligned > 0,
            "an uncorrected bold face should be a pixel off somewhere"
        );
    }

    #[test]
    fn any_exists_checks_paths() {
        assert!(!any_exists(&["/definitely/not/here.ttf"]));
        assert!(any_exists(&["/definitely/not/here.ttf", "/"]));
    }
}
