//! Pictures kept on disk (pages of the Images mode, covers) are stored as WebP: JPEG and PNG pages are decoded and
//! encoded again as lossy WebP, WebP files are kept as they are. Pages stay sharp at quality 85 and take a fraction of
//! the space of PNG pages.
use std::path::Path;

pub const QUALITY: f32 = 85.0;
/// Pages taller than this are not re-encoded (libwebp's limit is 16383 per side); they are kept as downloaded.
const MAX_SIDE: u32 = 16383;

pub fn is_webp(b: &[u8]) -> bool {
    b.len() > 12 && &b[..4] == b"RIFF" && &b[8..12] == b"WEBP"
}

fn is_png(b: &[u8]) -> bool {
    b.starts_with(&[0x89, b'P', b'N', b'G'])
}

fn is_jpeg(b: &[u8]) -> bool {
    b.starts_with(&[0xff, 0xd8, 0xff])
}

/// (pixels, width, height, has alpha)
fn decode(b: &[u8]) -> Option<(Vec<u8>, u32, u32, bool)> {
    if is_jpeg(b) {
        let mut d = jpeg_decoder::Decoder::new(b);
        // the size comes from the header: a picture too big to encode is not decoded at all (a very tall strip of
        // pages would take hundreds of megabytes of memory for nothing)
        d.read_info().ok()?;
        let info = d.info()?;
        if info.width as u32 > MAX_SIDE || info.height as u32 > MAX_SIDE {
            return None;
        }
        let px = d.decode().ok()?;
        let info = d.info()?;
        let (w, h) = (info.width as u32, info.height as u32);
        let rgb = match info.pixel_format {
            jpeg_decoder::PixelFormat::RGB24 => px,
            jpeg_decoder::PixelFormat::L8 => px.iter().flat_map(|&l| [l, l, l]).collect(),
            jpeg_decoder::PixelFormat::L16 => px.chunks_exact(2).flat_map(|c| [c[0], c[0], c[0]]).collect(),
            jpeg_decoder::PixelFormat::CMYK32 => px
                .chunks_exact(4)
                .flat_map(|c| {
                    let k = c[3] as u32;
                    [(c[0] as u32 * k / 255) as u8, (c[1] as u32 * k / 255) as u8, (c[2] as u32 * k / 255) as u8]
                })
                .collect(),
        };
        return Some((rgb, w, h, false));
    }
    if is_png(b) {
        let mut dec = png::Decoder::new(b);
        dec.set_transformations(png::Transformations::normalize_to_color8());
        let mut r = dec.read_info().ok()?;
        // as for JPEG: too big to encode, so not decoded
        if r.info().width > MAX_SIDE || r.info().height > MAX_SIDE {
            return None;
        }
        let mut buf = vec![0; r.output_buffer_size()];
        let info = r.next_frame(&mut buf).ok()?;
        buf.truncate(info.buffer_size());
        let (w, h) = (info.width, info.height);
        let (px, alpha) = match info.color_type {
            png::ColorType::Rgb => (buf, false),
            png::ColorType::Rgba => (buf, true),
            png::ColorType::Grayscale => (buf.iter().flat_map(|&l| [l, l, l]).collect(), false),
            png::ColorType::GrayscaleAlpha => (buf.chunks_exact(2).flat_map(|c| [c[0], c[0], c[0], c[1]]).collect(), true),
            png::ColorType::Indexed => return None,
        };
        return Some((px, w, h, alpha));
    }
    None
}

/// The picture as lossy WebP; None when it is already WebP or cannot be read (then the original is kept).
pub fn to_webp(b: &[u8]) -> Option<Vec<u8>> {
    if is_webp(b) {
        return None;
    }
    let (px, w, h, alpha) = decode(b)?;
    if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE {
        return None;
    }
    let enc = if alpha { libwebp::Encoder::from_rgba(&px, w, h) } else { libwebp::Encoder::from_rgb(&px, w, h) };
    let out = enc.encode(QUALITY).to_vec();
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// What is kept for a downloaded picture: its WebP version (or the bytes themselves when that is not possible).
pub fn for_disk(b: &[u8]) -> std::borrow::Cow<'_, [u8]> {
    match to_webp(b) {
        Some(w) => std::borrow::Cow::Owned(w),
        None => std::borrow::Cow::Borrowed(b),
    }
}

/// Writes a downloaded picture as WebP. Returns the bytes now on disk.
pub fn save(path: &Path, b: &[u8]) -> Vec<u8> {
    let data = for_disk(b).into_owned();
    if let Some(p) = path.parent() {
        let _ = std::fs::create_dir_all(p);
    }
    let _ = std::fs::write(path, &data);
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webp_is_kept_and_png_is_converted() {
        assert!(to_webp(b"RIFF\0\0\0\0WEBPVP8 ").is_none());
        let mut png_bytes = Vec::new();
        {
            let mut e = png::Encoder::new(&mut png_bytes, 4, 4);
            e.set_color(png::ColorType::Rgb);
            e.set_depth(png::BitDepth::Eight);
            let mut w = e.write_header().unwrap();
            w.write_image_data(&[200u8; 48]).unwrap();
        }
        let out = to_webp(&png_bytes).unwrap();
        assert!(is_webp(&out));
    }
}
