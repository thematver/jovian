//! Embedded fonts (OFL: Barlow, Barlow Condensed, JetBrains Mono), glyph rasterization with a
//! cache, and simple left-to-right text layout with kerning and letter spacing.

use std::cell::RefCell;
use std::collections::HashMap;
use tiny_skia::{Color, Mask, PixmapMut};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Face {
    Regular,
    Medium,
    Semi,
    Bold,
    Cond,
    CondSemi,
    CondBold,
    Mono,
    MonoSemi,
}

const FACE_DATA: [&[u8]; 9] = [
    include_bytes!("../fonts/Barlow-Regular.ttf"),
    include_bytes!("../fonts/Barlow-Medium.ttf"),
    include_bytes!("../fonts/Barlow-SemiBold.ttf"),
    include_bytes!("../fonts/Barlow-Bold.ttf"),
    include_bytes!("../fonts/BarlowCondensed-Medium.ttf"),
    include_bytes!("../fonts/BarlowCondensed-SemiBold.ttf"),
    include_bytes!("../fonts/BarlowCondensed-Bold.ttf"),
    include_bytes!("../fonts/JetBrainsMono-Medium.ttf"),
    include_bytes!("../fonts/JetBrainsMono-SemiBold.ttf"),
];

/// Font, size (logical px) and extra letter spacing (logical px).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Style {
    pub face: Face,
    pub size: f32,
    pub tracking: f32,
}

impl Style {
    pub const fn new(face: Face, size: f32, tracking: f32) -> Self {
        Style { face, size, tracking }
    }
}

struct Glyph {
    xmin: i32,
    ymin: i32,
    w: usize,
    h: usize,
    advance: f32,
    bitmap: Vec<u8>,
}

pub struct Fonts {
    fonts: Vec<fontdue::Font>,
    cache: RefCell<HashMap<(u8, u16, u32), Glyph>>,
}

impl Default for Fonts {
    fn default() -> Self {
        Self::new()
    }
}

impl Fonts {
    pub fn new() -> Self {
        let fonts = FACE_DATA
            .iter()
            .map(|d| fontdue::Font::from_bytes(*d, fontdue::FontSettings { scale: 40.0, ..Default::default() }).expect("font"))
            .collect();
        Fonts { fonts, cache: RefCell::new(HashMap::new()) }
    }

    fn font(&self, f: Face) -> &fontdue::Font {
        &self.fonts[f as usize]
    }

    fn with_glyph<R>(&self, face: Face, index: u16, px: f32, f: impl FnOnce(&Glyph) -> R) -> R {
        let key = (face as u8, index, (px * 8.0).round() as u32);
        let mut cache = self.cache.borrow_mut();
        let g = cache.entry(key).or_insert_with(|| {
            let (m, bitmap) = self.font(face).rasterize_indexed(index, px);
            Glyph { xmin: m.xmin, ymin: m.ymin, w: m.width, h: m.height, advance: m.advance_width, bitmap }
        });
        f(g)
    }

    /// Width of `s` in logical px at scale 1 (layout is done in device pixels for the scale used
    /// when drawing; the difference is below a pixel).
    pub fn measure(&self, s: &str, st: &Style) -> f32 {
        self.measure_scaled(s, st, 2.0) / 2.0
    }

    fn measure_scaled(&self, s: &str, st: &Style, scale: f32) -> f32 {
        let font = self.font(st.face);
        let px = st.size * scale;
        let mut w = 0.0;
        let mut prev: Option<u16> = None;
        let mut n = 0;
        for ch in s.chars() {
            let gi = font.lookup_glyph_index(ch);
            if let Some(p) = prev {
                w += font.horizontal_kern_indexed(p, gi, px).unwrap_or(0.0);
            }
            w += font.metrics_indexed(gi, px).advance_width;
            prev = Some(gi);
            n += 1;
        }
        if n > 1 {
            w += st.tracking * scale * (n - 1) as f32;
        }
        w
    }

    /// Height of capital letters in logical px.
    pub fn cap_height(&self, st: &Style) -> f32 {
        let m = self.font(st.face).metrics('H', st.size * 4.0);
        m.height as f32 / 4.0
    }

    /// Draws `s` with its baseline at logical (x, y).
    #[allow(clippy::too_many_arguments)]
    pub fn draw(&self, pix: &mut PixmapMut, scale: f32, s: &str, x: f32, y: f32, st: &Style, color: Color, clip: Option<&Mask>) {
        let font = self.font(st.face);
        let px = st.size * scale;
        let base = (y * scale).round() as i32;
        let mut pen = x * scale;
        let track = st.tracking * scale;
        let (pw, ph) = (pix.width() as i32, pix.height() as i32);
        let stride = pw as usize * 4;
        let cr = (color.red() * color.alpha() * 255.0) as u32;
        let cg = (color.green() * color.alpha() * 255.0) as u32;
        let cb = (color.blue() * color.alpha() * 255.0) as u32;
        let ca = (color.alpha() * 255.0) as u32;
        let mask = clip.map(|m| (m.data(), m.width() as usize));
        let data = pix.data_mut();
        let mut prev: Option<u16> = None;
        for ch in s.chars() {
            let gi = font.lookup_glyph_index(ch);
            if let Some(p) = prev {
                pen += font.horizontal_kern_indexed(p, gi, px).unwrap_or(0.0);
            }
            let advance = self.with_glyph(st.face, gi, px, |g| {
                let gx = (pen + g.xmin as f32).round() as i32;
                let gy = base - g.h as i32 - g.ymin;
                for row in 0..g.h as i32 {
                    let yy = gy + row;
                    if yy < 0 || yy >= ph {
                        continue;
                    }
                    for col in 0..g.w as i32 {
                        let xx = gx + col;
                        if xx < 0 || xx >= pw {
                            continue;
                        }
                        let mut cov = g.bitmap[row as usize * g.w + col as usize] as u32;
                        if cov == 0 {
                            continue;
                        }
                        if let Some((md, mw)) = mask {
                            cov = cov * md[yy as usize * mw + xx as usize] as u32 / 255;
                            if cov == 0 {
                                continue;
                            }
                        }
                        let o = yy as usize * stride + xx as usize * 4;
                        let a = ca * cov / 255;
                        let inv = 255 - a;
                        let p = &mut data[o..o + 4];
                        p[0] = ((cr * cov / 255) + p[0] as u32 * inv / 255).min(255) as u8;
                        p[1] = ((cg * cov / 255) + p[1] as u32 * inv / 255).min(255) as u8;
                        p[2] = ((cb * cov / 255) + p[2] as u32 * inv / 255).min(255) as u8;
                        p[3] = (a + p[3] as u32 * inv / 255).min(255) as u8;
                    }
                }
                g.advance
            });
            pen += advance + track;
            prev = Some(gi);
        }
    }
}
