//! How large an image is displayed. Pure geometry, shared by the layout (reserved row height)
//! and the painter, so the two can never disagree.

use egui::Vec2;

/// Space kept below an image inside its reserved row.
pub const PAD: f32 = 8.0;
/// Default (automatic) sizing never makes an image taller than this.
pub const MAX_AUTO_HEIGHT: f32 = 480.0;
pub const MIN_PCT: u16 = 20;
pub const MAX_PCT: u16 = 100;

/// Displayed size of an image of `natural` size in a content area `content_w` wide.
///
/// - No annotation: natural size, shrunk to fit the width and `MAX_AUTO_HEIGHT`, never enlarged.
/// - `{width=P%}`: `P` clamped to 20..=100 percent of the content width (so never wider than the
///   content area); the image is scaled up or down to it and keeps its aspect ratio.
pub fn display(natural: Vec2, content_w: f32, pct: Option<u16>) -> Vec2 {
    let natural = Vec2::new(natural.x.max(1.0), natural.y.max(1.0));
    let content_w = content_w.max(1.0);
    match pct {
        None => {
            let scale = ((content_w - PAD) / natural.x)
                .min(MAX_AUTO_HEIGHT / natural.y)
                .clamp(0.01, 1.0)
                .min(content_w / natural.x);
            natural * scale
        }
        Some(p) => {
            let p = f32::from(p.clamp(MIN_PCT, MAX_PCT));
            let w = (content_w * p / 100.0).clamp(1.0, content_w);
            Vec2::new(w, w * natural.y / natural.x)
        }
    }
}

/// The percentage a slider shows for an image: the annotation, or what automatic sizing gives.
pub fn effective_pct(natural: Vec2, content_w: f32, pct: Option<u16>) -> u16 {
    match pct {
        Some(p) => p.clamp(MIN_PCT, MAX_PCT),
        None => {
            let w = display(natural, content_w, None).x;
            ((w / content_w.max(1.0) * 100.0).round() as u16).clamp(MIN_PCT, MAX_PCT)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_size_is_unchanged_from_before() {
        // Fits: natural size.
        assert_eq!(
            display(Vec2::new(200.0, 100.0), 700.0, None),
            Vec2::new(200.0, 100.0)
        );
        // Too wide: fits width minus padding.
        let d = display(Vec2::new(1600.0, 800.0), 700.0, None);
        assert!((d.x - (700.0 - PAD)).abs() < 0.01);
        // Too tall: capped at 480.
        let d = display(Vec2::new(500.0, 2000.0), 700.0, None);
        assert!((d.y - 480.0).abs() < 0.01);
    }

    #[test]
    fn annotated_width_is_a_percentage_of_the_content_width_with_aspect_kept() {
        let nat = Vec2::new(1000.0, 500.0);
        for (p, w) in [(60, 420.0), (100, 700.0), (20, 140.0)] {
            let d = display(nat, 700.0, Some(p));
            assert!((d.x - w).abs() < 0.01, "{p}");
            assert!((d.x / d.y - 2.0).abs() < 1e-4, "aspect");
        }
        // Small images are enlarged to the requested width, so the slider always has an effect.
        assert!((display(Vec2::new(100.0, 50.0), 700.0, Some(50)).x - 350.0).abs() < 0.01);
    }

    #[test]
    fn limits_and_narrow_areas() {
        let nat = Vec2::new(300.0, 300.0);
        assert_eq!(
            display(nat, 700.0, Some(5)).x,
            display(nat, 700.0, Some(20)).x
        );
        assert_eq!(display(nat, 700.0, Some(900)).x, 700.0);
        assert_eq!(display(nat, 700.0, Some(0)).x, 140.0);
        for w in [0.0, 1.0, 10.0, 40.0] {
            for p in [None, Some(20), Some(100)] {
                let d = display(nat, w, p);
                assert!(
                    d.x >= 0.0 && d.y > 0.0 && d.x <= w.max(1.0) + 1e-3,
                    "{w} {p:?} {d:?}"
                );
            }
        }
        // Degenerate natural sizes cannot divide by zero.
        assert!(display(Vec2::ZERO, 700.0, Some(50)).y.is_finite());
    }

    #[test]
    fn effective_percentage_for_the_slider() {
        let nat = Vec2::new(1600.0, 800.0);
        assert_eq!(effective_pct(nat, 700.0, Some(60)), 60);
        assert_eq!(effective_pct(nat, 700.0, Some(300)), 100);
        assert_eq!(effective_pct(nat, 700.0, None), 99);
        assert_eq!(effective_pct(Vec2::new(70.0, 70.0), 700.0, None), 20);
    }
}
