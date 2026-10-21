//! The small picture of a page that the browser panel lists: a frame reduced to at most
//! `MAX_WIDTH` pixels and encoded as PNG.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, ImageEncoder};

/// The widest a thumbnail gets, in pixels. The panel draws it about this size.
pub const MAX_WIDTH: u32 = 240;

/// Reduces one BGRA frame and returns it as base64 PNG, or None when the frame is empty or
/// does not match its size.
pub fn encode(width: u32, height: u32, bgra: &[u8]) -> Option<String> {
    let expected = width as usize * height as usize * 4;
    if width == 0 || height == 0 || bgra.len() != expected {
        return None;
    }
    let step = width.div_ceil(MAX_WIDTH).max(1);
    let (out_w, out_h) = (width.div_ceil(step), height.div_ceil(step));
    let mut rgba = Vec::with_capacity(out_w as usize * out_h as usize * 4);
    for oy in 0..out_h {
        for ox in 0..out_w {
            rgba.extend_from_slice(&average_block(width, height, bgra, step, ox, oy));
        }
    }
    let mut png = Vec::new();
    PngEncoder::new(&mut png)
        .write_image(&rgba, out_w, out_h, ExtendedColorType::Rgba8)
        .ok()?;
    Some(STANDARD.encode(png))
}

/// The average colour of one `step` by `step` block, converted from BGRA to RGBA.
fn average_block(width: u32, height: u32, bgra: &[u8], step: u32, ox: u32, oy: u32) -> [u8; 4] {
    let mut sum = [0u32; 3];
    let mut count = 0u32;
    for y in (oy * step)..((oy + 1) * step).min(height) {
        for x in (ox * step)..((ox + 1) * step).min(width) {
            let at = (y as usize * width as usize + x as usize) * 4;
            sum[0] += u32::from(bgra[at + 2]);
            sum[1] += u32::from(bgra[at + 1]);
            sum[2] += u32::from(bgra[at]);
            count += 1;
        }
    }
    let average = |total: u32| (total / count.max(1)) as u8;
    [average(sum[0]), average(sum[1]), average(sum[2]), 255]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(encoded: &str) -> Vec<u8> {
        STANDARD.decode(encoded).expect("base64")
    }

    #[test]
    fn a_small_frame_keeps_its_size() {
        let frame = vec![255u8; 4 * 4 * 4];
        let png = encode(4, 4, &frame).expect("encoded");
        let bytes = decode(&png);
        assert_eq!(&bytes[..4], &[0x89, b'P', b'N', b'G']);
        let image = image::load_from_memory(&bytes).expect("a PNG");
        assert_eq!((image.width(), image.height()), (4, 4));
    }

    #[test]
    fn a_wide_frame_is_reduced_to_at_most_the_thumbnail_width() {
        let frame = vec![10u8; 800 * 500 * 4];
        let png = encode(800, 500, &frame).expect("encoded");
        let image = image::load_from_memory(&decode(&png)).expect("a PNG");
        assert!(image.width() <= MAX_WIDTH, "{}", image.width());
        assert_eq!(image.height(), image.width() * 500 / 800);
    }

    #[test]
    fn colours_are_averaged_and_turned_from_bgra_to_rgba() {
        // Two pixels of pure blue (BGRA 255,0,0,255) on a 2x1 frame: the thumbnail keeps blue.
        let frame = [255u8, 0, 0, 255, 255, 0, 0, 255];
        let png = encode(2, 1, &frame).expect("encoded");
        let image = image::load_from_memory(&decode(&png))
            .expect("a PNG")
            .to_rgba8();
        assert_eq!(image.get_pixel(0, 0).0, [0, 0, 255, 255]);
    }

    #[test]
    fn a_frame_that_does_not_match_its_size_is_refused() {
        assert_eq!(encode(4, 4, &[0u8; 10]), None);
        assert_eq!(encode(0, 4, &[]), None);
    }
}
