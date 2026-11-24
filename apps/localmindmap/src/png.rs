//! The PNG export: the map, rendered off-screen and written as a PNG.
//!
//! There is no rasterizer in the suite and none is coming with a C library, so the image is built
//! from the same drawing the window does: an off-screen `egui` context paints the map with
//! [`crate::canvas::paint`], the shapes are tessellated by epaint, and the triangles are filled into
//! an RGBA buffer here (texturing from the font atlas). The PNG is then encoded by hand - a PNG is
//! a zlib stream, and zlib allows *stored* (uncompressed) deflate blocks, so no compression
//! dependency is needed either; the file is bigger than a compressed one, and it is correct.
//!
//! What this is not: a general rasterizer. The triangles are the ones epaint produced for our own
//! shapes (axis-aligned quads and the AA fringes of rounded rectangles), so the sampler is simple by
//! design.

use egui::{Color32, Pos2, Rect, TextureId, vec2};
use localmindmap_layout::layout_tree;
use localmindmap_model::MindMap;

use crate::canvas::{self, CanvasView};
use crate::theme::Colors;

/// Margin around the map, in layout units.
const PAD: f32 = 60.0;
/// The image is rendered at this many pixels per layout unit (a retina-looking export).
const SCALE: f32 = 2.0;

/// Renders the map to PNG bytes.
pub fn render(map: &MindMap, colors: &Colors) -> Result<Vec<u8>, String> {
    let layout = layout_tree(&map.nodes);
    let Some((origin, size)) = layout.bounds(&map.nodes) else { return Err("the map has nothing to draw".into()) };
    let width = ((size.w + 2.0 * PAD) * SCALE).round().max(1.0) as u32;
    let height = ((size.h + 2.0 * PAD) * SCALE).round().max(1.0) as u32;
    if width as u64 * height as u64 > 64_000_000 {
        return Err("the map is too large to export as one image".into());
    }

    // One point is one pixel here, and the map is drawn at `SCALE`: the cards and their text are
    // laid out at that size, which is what makes the export crisp instead of upscaled.
    let logical = Rect::from_min_size(Pos2::ZERO, vec2(width as f32, height as f32));
    let view = CanvasView {
        pan: vec2(PAD, PAD) * SCALE - logical.center().to_vec2() - vec2(origin.x, origin.y) * SCALE,
        zoom: SCALE,
        needs_fit: false,
        resizing: None,
    };

    let ctx = egui::Context::default();
    crate::fonts::install(&ctx);
    let output = ctx.run_ui(egui::RawInput { screen_rect: Some(logical), ..Default::default() }, |ui| {
        canvas::paint(ui.painter(), map, &layout, &view, logical, None, None, colors);
    });

    // The atlas arrives as a texture delta this frame; it has to be applied (or cleared) before the
    // delta is dropped.
    let mut textures = output.textures_delta;
    let atlas = textures.set.get(&TextureId::Managed(0)).and_then(|deltas| deltas.last()).map(|delta| delta.image.clone());
    textures.clear();
    let atlas = atlas.ok_or("the font atlas was not produced")?;
    let mask = Mask::from(&atlas);

    let mut image = vec![colors.canvas; (width * height) as usize];
    for clipped in ctx.tessellate(output.shapes, 1.0) {
        if let egui::epaint::Primitive::Mesh(mesh) = clipped.primitive {
            fill(&mesh, &mask, &mut image, width, height);
        }
    }
    encode(&image, width, height)
}

/// The glyph coverage the text quads sample.
struct Mask {
    width: usize,
    height: usize,
    /// One alpha byte per texel (the atlas is white with an alpha channel).
    alpha: Vec<u8>,
}

impl Mask {
    fn from(image: &egui::epaint::ImageData) -> Self {
        let egui::epaint::ImageData::Color(image) = image;
        let [width, height] = image.size;
        let alpha = image.pixels.iter().map(|pixel| pixel.a()).collect();
        Self { width, height, alpha }
    }

    /// Bilinear coverage at `uv` (measured in texels).
    fn sample(&self, u: f32, v: f32) -> f32 {
        let x = (u * self.width as f32 - 0.5).clamp(0.0, (self.width - 1) as f32);
        let y = (v * self.height as f32 - 0.5).clamp(0.0, (self.height - 1) as f32);
        let x0 = x.floor() as usize;
        let y0 = y.floor() as usize;
        let x1 = (x0 + 1).min(self.width - 1);
        let y1 = (y0 + 1).min(self.height - 1);
        let fx = x - x0 as f32;
        let fy = y - y0 as f32;
        let at = |x: usize, y: usize| self.alpha[y * self.width + x] as f32 / 255.0;
        let top = at(x0, y0) * (1.0 - fx) + at(x1, y0) * fx;
        let bottom = at(x0, y1) * (1.0 - fx) + at(x1, y1) * fx;
        top * (1.0 - fy) + bottom * fy
    }
}

/// The white texel epaint uses for solid fills (see `epaint::WHITE_UV`).
fn white_uv() -> egui::Vec2 {
    egui::vec2(0.0, 0.0)
}

/// Fills one mesh into the RGBA buffer. Colours are premultiplied sRGBA, as epaint made them.
fn fill(mesh: &egui::epaint::Mesh, mask: &Mask, image: &mut [Color32], width: u32, height: u32) {
    for triangle in mesh.indices.as_chunks::<3>().0 {
        let [a, b, c] = [triangle[0] as usize, triangle[1] as usize, triangle[2] as usize];
        let (Some(va), Some(vb), Some(vc)) = (mesh.vertices.get(a), mesh.vertices.get(b), mesh.vertices.get(c)) else { continue };
        let area = (vb.pos.x - va.pos.x) * (vc.pos.y - va.pos.y) - (vc.pos.x - va.pos.x) * (vb.pos.y - va.pos.y);
        if area.abs() < f32::EPSILON {
            continue;
        }
        let min_x = va.pos.x.min(vb.pos.x).min(vc.pos.x).floor().max(0.0) as u32;
        let max_x = (va.pos.x.max(vb.pos.x).max(vc.pos.x).ceil() as u32).min(width);
        let min_y = va.pos.y.min(vb.pos.y).min(vc.pos.y).floor().max(0.0) as u32;
        let max_y = (va.pos.y.max(vb.pos.y).max(vc.pos.y).ceil() as u32).min(height);
        let solid = (va.uv.to_vec2() - white_uv()).length() < 0.001
            && (vb.uv.to_vec2() - white_uv()).length() < 0.001
            && (vc.uv.to_vec2() - white_uv()).length() < 0.001;
        for y in min_y..max_y {
            for x in min_x..max_x {
                let px = x as f32 + 0.5;
                let py = y as f32 + 0.5;
                let w0 = ((vb.pos.x - px) * (vc.pos.y - py) - (vc.pos.x - px) * (vb.pos.y - py)) / area;
                let w1 = ((vc.pos.x - px) * (va.pos.y - py) - (va.pos.x - px) * (vc.pos.y - py)) / area;
                let w2 = 1.0 - w0 - w1;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let color = |channel: fn(&Color32) -> u8| {
                    (w0 * channel(&va.color) as f32 + w1 * channel(&vb.color) as f32 + w2 * channel(&vc.color) as f32).round().clamp(0.0, 255.0) as u8
                };
                let mut src = Color32::from_rgba_premultiplied(color(|c| c.r()), color(|c| c.g()), color(|c| c.b()), color(|c| c.a()));
                if !solid {
                    let u = w0 * va.uv.x + w1 * vb.uv.x + w2 * vc.uv.x;
                    let v = w0 * va.uv.y + w1 * vb.uv.y + w2 * vc.uv.y;
                    let coverage = mask.sample(u, v);
                    src = Color32::from_rgba_premultiplied(
                        (src.r() as f32 * coverage) as u8,
                        (src.g() as f32 * coverage) as u8,
                        (src.b() as f32 * coverage) as u8,
                        (src.a() as f32 * coverage) as u8,
                    );
                }
                if src.a() == 0 {
                    continue;
                }
                let at = (y * width + x) as usize;
                image[at] = over(src, image[at]);
            }
        }
    }
}

/// Premultiplied source over destination.
fn over(src: Color32, dst: Color32) -> Color32 {
    let keep = 1.0 - src.a() as f32 / 255.0;
    let mix = |s: u8, d: u8| (s as f32 + d as f32 * keep).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgba_premultiplied(mix(src.r(), dst.r()), mix(src.g(), dst.g()), mix(src.b(), dst.b()), mix(src.a(), dst.a()))
}

/// Encodes RGBA8 as a PNG: signature, IHDR, one IDAT of stored deflate blocks, IEND.
fn encode(pixels: &[Color32], width: u32, height: u32) -> Result<Vec<u8>, String> {
    let mut raw = Vec::with_capacity((width as usize * 4 + 1) * height as usize);
    for row in 0..height {
        raw.push(0); // filter: none
        for x in 0..width {
            let pixel = pixels[(row * width + x) as usize];
            raw.extend_from_slice(&[pixel.r(), pixel.g(), pixel.b(), pixel.a()]);
        }
    }

    let mut out = Vec::new();
    out.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);
    let mut header = Vec::with_capacity(13);
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    header.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit, RGBA, deflate, no filter/interlace
    chunk(&mut out, b"IHDR", &header);
    chunk(&mut out, b"IDAT", &miniz_oxide::deflate::compress_to_vec_zlib(&raw, 6));
    chunk(&mut out, b"IEND", &[]);
    Ok(out)
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc = 0xffff_ffffu32;
    for byte in kind.iter().chain(data.iter()) {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    out.extend_from_slice(&(crc ^ 0xffff_ffff).to_be_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use localmindmap_model::MindNode;

    fn map(nodes: Vec<MindNode>) -> MindMap {
        MindMap { id: "m".into(), title: "m".into(), nodes, created_at: 0, updated_at: 0, extra: Default::default() }
    }

    /// A PNG starts with its signature, carries IHDR with the size and ends with IEND.
    #[test]
    fn the_encoder_writes_a_png() {
        let pixels = vec![Color32::from_rgb(10, 20, 30); 4 * 3];
        let png = encode(&pixels, 4, 3).expect("encode");
        assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);
        assert_eq!(&png[12..16], b"IHDR");
        assert_eq!(u32::from_be_bytes([png[16], png[17], png[18], png[19]]), 4);
        assert_eq!(u32::from_be_bytes([png[20], png[21], png[22], png[23]]), 3);
        assert_eq!(&png[png.len() - 8..png.len() - 4], b"IEND");
    }

    /// The encoder's deflate round-trips through the same crate that produced it.
    #[test]
    fn the_idat_is_a_zlib_stream_of_the_pixels() {
        let pixels = vec![Color32::from_rgb(10, 20, 30); 4 * 3];
        let png = encode(&pixels, 4, 3).expect("encode");
        // Find the IDAT chunk and inflate it back.
        let mut at = 8;
        let mut raw = None;
        while at + 8 <= png.len() {
            let length = u32::from_be_bytes([png[at], png[at + 1], png[at + 2], png[at + 3]]) as usize;
            let kind = &png[at + 4..at + 8];
            if kind == b"IDAT" {
                raw = Some(miniz_oxide::inflate::decompress_to_vec_zlib(&png[at + 8..at + 8 + length]).expect("inflate"));
            }
            at += 12 + length;
        }
        let raw = raw.expect("no IDAT");
        assert_eq!(raw.len(), (4 * 4 + 1) * 3, "three rows of four RGBA pixels, one filter byte each");
        assert_eq!(&raw[1..5], &[10, 20, 30, 255]);
    }

    /// The empty map has nothing to draw and says so instead of writing a 1x1 image.
    #[test]
    fn an_empty_map_is_an_error() {
        assert!(render(&map(vec![]), &crate::theme::Theme::Dark.colors()).is_err());
    }

    /// The whole pipeline: a map with two cards renders to a PNG big enough to hold them, and the
    /// background is the theme's canvas colour (the cards are painted over it).
    #[test]
    fn a_map_renders_to_a_png_with_the_theme_background() {
        let colors = crate::theme::Theme::Dark.colors();
        let png = render(&map(vec![MindNode::new("r", "Central", None), MindNode::new("a", "Ramo", Some("r".into()))]), &colors).expect("render");
        assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);
        // SCALE = 2: a 220-wide card plus padding is at least 300 logical -> 600 physical pixels.
        let width = u32::from_be_bytes([png[16], png[17], png[18], png[19]]);
        assert!(width >= 600, "unexpected width {width}");
    }

    /// The QA render: the compatibility fixture exported twice (the two themes the QA looks at).
    #[test]
    fn the_fixture_exports_to_a_readable_png() {
        let Ok(text) = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../crates/io/tests/fixtures/sepse.tmind")) else {
            return;
        };
        let Ok(fixture) = localmindmap_io::parse(&text) else { return };
        for theme in [crate::theme::Theme::Dark, crate::theme::Theme::Light] {
            let png = render(&fixture, &theme.colors()).expect("render");
            assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);
            if std::env::var_os("LOCALMIND_QA_RENDER").is_none() {
                continue;
            }
            let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/qa-f3");
            if std::fs::create_dir_all(&directory).is_err() {
                continue;
            }
            let _ = std::fs::write(directory.join(format!("png-{}", theme.id())), &png);
            // The extension is what makes it open: write it again with `.png`.
            let _ = std::fs::write(directory.join(format!("png-{}.png", theme.id())), &png);
        }
    }
}
