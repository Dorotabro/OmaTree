//! Image encoding and decoding. Where the bytes are kept is the business of
//! [`crate::attachments::AttachmentStore`]; this module never touches files.

use egui::ColorImage;
use image::ImageEncoder;

/// Encode `img` as PNG bytes (the format of pasted screenshots).
pub fn encode_png(img: &ColorImage) -> Result<Vec<u8>, String> {
    let [w, h] = img.size;
    let mut rgba = Vec::with_capacity(w * h * 4);
    for p in &img.pixels {
        rgba.extend_from_slice(&p.to_srgba_unmultiplied());
    }
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(&rgba, w as u32, h as u32, image::ExtendedColorType::Rgba8)
        .map_err(|e| format!("PNG encode failed: {e}"))?;
    Ok(png)
}

/// Decode image bytes (any format the `image` crate was built with) for display.
pub fn decode(bytes: &[u8]) -> Result<ColorImage, String> {
    let img = image::load_from_memory(bytes)
        .map_err(|e| e.to_string())?
        .to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    Ok(ColorImage::from_rgba_unmultiplied(size, img.as_raw()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Color32;

    #[test]
    fn encode_decode_roundtrip() {
        let img = ColorImage::new([3, 2], vec![Color32::from_rgb(10, 20, 30); 6]);
        let back = decode(&encode_png(&img).unwrap()).unwrap();
        assert_eq!(back.size, [3, 2]);
        assert_eq!(back.pixels[0], Color32::from_rgb(10, 20, 30));
        assert!(decode(b"not an image").is_err());
        assert!(decode(&[]).is_err());
    }
}
