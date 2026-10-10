//! A thin drawing layer over tiny-skia. All coordinates are logical points; the canvas scales
//! them to device pixels. `swap_rb` renders straight into a BGRA (softbuffer) buffer.

use crate::text::{Fonts, Style};
pub use tiny_skia::{FillRule, Path, PathBuilder};
use tiny_skia::{
    Color, GradientStop, LinearGradient, Mask, Paint, PixmapMut, PixmapPaint, PixmapRef, Point, RadialGradient, Rect,
    Shader, SpreadMode, Stroke, Transform,
};

/// Straight (non-premultiplied) sRGB color.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Rgba {
    pub const fn hex(v: u32) -> Self {
        Rgba {
            r: ((v >> 16) & 0xFF) as f32 / 255.0,
            g: ((v >> 8) & 0xFF) as f32 / 255.0,
            b: (v & 0xFF) as f32 / 255.0,
            a: 1.0,
        }
    }

    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Rgba { r, g, b, a }
    }

    pub const fn white(a: f32) -> Self {
        Rgba { r: 1.0, g: 1.0, b: 1.0, a }
    }

    pub const fn black(a: f32) -> Self {
        Rgba { r: 0.0, g: 0.0, b: 0.0, a }
    }

    pub fn with_alpha(self, a: f32) -> Self {
        Rgba { a, ..self }
    }

    pub fn mul_alpha(self, k: f32) -> Self {
        Rgba { a: self.a * k, ..self }
    }

    /// Mixes towards `o` by `t`.
    pub fn mix(self, o: Rgba, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        Rgba {
            r: self.r + (o.r - self.r) * t,
            g: self.g + (o.g - self.g) * t,
            b: self.b + (o.b - self.b) * t,
            a: self.a + (o.a - self.a) * t,
        }
    }

    pub fn lighten(self, t: f32) -> Self {
        self.mix(Rgba::white(self.a), t)
    }

    pub fn darken(self, t: f32) -> Self {
        self.mix(Rgba::black(self.a), t)
    }
}

/// A color ramp for gradients: (position 0...1, color).
pub type Stops = Vec<(f32, Rgba)>;

#[derive(Debug, Clone)]
pub enum Fill {
    Solid(Rgba),
    /// From (x0, y0) to (x1, y1) in logical coordinates.
    Linear(f32, f32, f32, f32, Stops),
    /// Centre (x, y) and radius.
    Radial(f32, f32, f32, Stops),
}

impl From<Rgba> for Fill {
    fn from(c: Rgba) -> Self {
        Fill::Solid(c)
    }
}

pub fn vgrad(y0: f32, y1: f32, top: Rgba, bottom: Rgba) -> Fill {
    Fill::Linear(0.0, y0, 0.0, y1, vec![(0.0, top), (1.0, bottom)])
}

pub fn hgrad(x0: f32, x1: f32, left: Rgba, right: Rgba) -> Fill {
    Fill::Linear(x0, 0.0, x1, 0.0, vec![(0.0, left), (1.0, right)])
}

/// Logical rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct R {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl R {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        R { x, y, w, h }
    }
    pub fn right(&self) -> f32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
    pub fn cx(&self) -> f32 {
        self.x + self.w * 0.5
    }
    pub fn cy(&self) -> f32 {
        self.y + self.h * 0.5
    }
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
    pub fn inset(&self, d: f32) -> R {
        R::new(self.x + d, self.y + d, self.w - 2.0 * d, self.h - 2.0 * d)
    }
    pub fn inset_xy(&self, dx: f32, dy: f32) -> R {
        R::new(self.x + dx, self.y + dy, self.w - 2.0 * dx, self.h - 2.0 * dy)
    }
    pub fn offset(&self, dx: f32, dy: f32) -> R {
        R::new(self.x + dx, self.y + dy, self.w, self.h)
    }
    pub fn union(&self, o: &R) -> R {
        let x = self.x.min(o.x);
        let y = self.y.min(o.y);
        R::new(x, y, self.right().max(o.right()) - x, self.bottom().max(o.bottom()) - y)
    }
    pub fn intersects(&self, o: &R) -> bool {
        self.x < o.right() && o.x < self.right() && self.y < o.bottom() && o.y < self.bottom()
    }
}

pub struct Canvas<'a> {
    pub pix: PixmapMut<'a>,
    pub scale: f32,
    pub swap_rb: bool,
    pub fonts: &'a Fonts,
    ts: Transform,
    clip: Option<Mask>,
}

pub fn rrect_path(r: R, rad: f32) -> Option<Path> {
    let rad = rad.min(r.w * 0.5).min(r.h * 0.5).max(0.0);
    let mut pb = PathBuilder::new();
    if rad <= 0.01 {
        pb.push_rect(Rect::from_xywh(r.x, r.y, r.w.max(0.01), r.h.max(0.01))?);
        return pb.finish();
    }
    let k = 0.552_284_8 * rad;
    let (x0, y0, x1, y1) = (r.x, r.y, r.x + r.w, r.y + r.h);
    pb.move_to(x0 + rad, y0);
    pb.line_to(x1 - rad, y0);
    pb.cubic_to(x1 - rad + k, y0, x1, y0 + rad - k, x1, y0 + rad);
    pb.line_to(x1, y1 - rad);
    pb.cubic_to(x1, y1 - rad + k, x1 - rad + k, y1, x1 - rad, y1);
    pb.line_to(x0 + rad, y1);
    pb.cubic_to(x0 + rad - k, y1, x0, y1 - rad + k, x0, y1 - rad);
    pb.line_to(x0, y0 + rad);
    pb.cubic_to(x0, y0 + rad - k, x0 + rad - k, y0, x0 + rad, y0);
    pb.close();
    pb.finish()
}

pub fn circle_path(cx: f32, cy: f32, r: f32) -> Option<Path> {
    PathBuilder::from_circle(cx, cy, r.max(0.01))
}

impl<'a> Canvas<'a> {
    pub fn new(pix: PixmapMut<'a>, scale: f32, swap_rb: bool, fonts: &'a Fonts) -> Self {
        Canvas { pix, scale, swap_rb, fonts, ts: Transform::from_scale(scale, scale), clip: None }
    }

    pub fn color(&self, c: Rgba) -> Color {
        let (r, b) = if self.swap_rb { (c.b, c.r) } else { (c.r, c.b) };
        Color::from_rgba(r.clamp(0.0, 1.0), c.g.clamp(0.0, 1.0), b.clamp(0.0, 1.0), c.a.clamp(0.0, 1.0)).unwrap_or(Color::BLACK)
    }

    fn stops(&self, s: &Stops) -> Vec<GradientStop> {
        s.iter().map(|(p, c)| GradientStop::new(*p, self.color(*c))).collect()
    }

    fn paint(&self, f: &Fill) -> Paint<'static> {
        let shader = match f {
            Fill::Solid(c) => Shader::SolidColor(self.color(*c)),
            Fill::Linear(x0, y0, x1, y1, s) => {
                LinearGradient::new(Point::from_xy(*x0, *y0), Point::from_xy(*x1, *y1), self.stops(s), SpreadMode::Pad, Transform::identity())
                    .unwrap_or_else(|| Shader::SolidColor(self.color(s.first().map(|x| x.1).unwrap_or(Rgba::black(0.0)))))
            }
            Fill::Radial(x, y, r, s) => {
                RadialGradient::new(Point::from_xy(*x, *y), Point::from_xy(*x, *y), *r, self.stops(s), SpreadMode::Pad, Transform::identity())
                    .unwrap_or_else(|| Shader::SolidColor(self.color(s.first().map(|x| x.1).unwrap_or(Rgba::black(0.0)))))
            }
        };
        Paint { shader, anti_alias: true, ..Default::default() }
    }

    /// Restricts drawing to a logical rectangle (until `clear_clip`).
    pub fn set_clip(&mut self, r: R) {
        let (w, h) = (self.pix.width(), self.pix.height());
        if let Some(mut m) = Mask::new(w, h) {
            if let Some(p) = rrect_path(r, 0.0) {
                m.fill_path(&p, FillRule::Winding, false, self.ts);
            }
            self.clip = Some(m);
        }
    }

    pub fn clear_clip(&mut self) {
        self.clip = None;
    }

    pub fn fill_path(&mut self, p: &Path, f: impl Into<Fill>) {
        let paint = self.paint(&f.into());
        self.pix.fill_path(p, &paint, FillRule::Winding, self.ts, self.clip.as_ref());
    }

    pub fn stroke_path(&mut self, p: &Path, width: f32, f: impl Into<Fill>) {
        let paint = self.paint(&f.into());
        let stroke = Stroke { width, line_cap: tiny_skia::LineCap::Round, line_join: tiny_skia::LineJoin::Round, ..Default::default() };
        self.pix.stroke_path(p, &paint, &stroke, self.ts, self.clip.as_ref());
    }

    pub fn stroke_path_butt(&mut self, p: &Path, width: f32, f: impl Into<Fill>) {
        let paint = self.paint(&f.into());
        let stroke = Stroke { width, ..Default::default() };
        self.pix.stroke_path(p, &paint, &stroke, self.ts, self.clip.as_ref());
    }

    pub fn rect(&mut self, r: R, f: impl Into<Fill>) {
        if let Some(p) = rrect_path(r, 0.0) {
            self.fill_path(&p, f);
        }
    }

    pub fn rrect(&mut self, r: R, rad: f32, f: impl Into<Fill>) {
        if let Some(p) = rrect_path(r, rad) {
            self.fill_path(&p, f);
        }
    }

    pub fn rrect_stroke(&mut self, r: R, rad: f32, width: f32, f: impl Into<Fill>) {
        // keep hairlines crisp: stroke on the pixel grid of the inset rectangle
        let h = width * 0.5;
        if let Some(p) = rrect_path(r.inset(h), (rad - h).max(0.0)) {
            self.stroke_path_butt(&p, width, f);
        }
    }

    pub fn circle(&mut self, cx: f32, cy: f32, r: f32, f: impl Into<Fill>) {
        if let Some(p) = circle_path(cx, cy, r) {
            self.fill_path(&p, f);
        }
    }

    pub fn circle_stroke(&mut self, cx: f32, cy: f32, r: f32, width: f32, f: impl Into<Fill>) {
        if let Some(p) = circle_path(cx, cy, r) {
            self.stroke_path_butt(&p, width, f);
        }
    }

    pub fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, width: f32, c: Rgba) {
        let mut pb = PathBuilder::new();
        pb.move_to(x0, y0);
        pb.line_to(x1, y1);
        if let Some(p) = pb.finish() {
            self.stroke_path_butt(&p, width, c);
        }
    }

    /// Horizontal hairline snapped to device pixels.
    pub fn hline(&mut self, x0: f32, x1: f32, y: f32, width: f32, c: Rgba) {
        let s = self.scale;
        let yy = ((y * s).floor() + 0.5 * (width * s).round().max(1.0)) / s;
        self.line(x0, yy, x1, yy, (width * s).round().max(1.0) / s, c);
    }

    /// Vertical hairline snapped to device pixels.
    pub fn vline(&mut self, x: f32, y0: f32, y1: f32, width: f32, c: Rgba) {
        let s = self.scale;
        let xx = ((x * s).floor() + 0.5 * (width * s).round().max(1.0)) / s;
        self.line(xx, y0, xx, y1, (width * s).round().max(1.0) / s, c);
    }

    /// Soft drop shadow under a rounded rectangle (layered approximation of a blur).
    pub fn shadow(&mut self, r: R, rad: f32, blur: f32, dy: f32, c: Rgba) {
        let steps = ((blur * self.scale) as usize).clamp(2, 8);
        for i in 0..steps {
            let t = (i as f32 + 0.5) / steps as f32;
            let spread = blur * (1.0 - t);
            let a = c.a / steps as f32 * (0.6 + 0.8 * t);
            self.rrect(r.offset(0.0, dy).inset(-spread), rad + spread, c.with_alpha(a));
        }
    }

    /// Glow around a point (radial falloff).
    pub fn glow(&mut self, cx: f32, cy: f32, r: f32, c: Rgba) {
        self.circle(cx, cy, r, Fill::Radial(cx, cy, r, vec![(0.0, c), (0.35, c.mul_alpha(0.45)), (1.0, c.with_alpha(0.0))]));
    }

    /// Copies a same-sized pixmap (a cached layer) over the whole canvas.
    pub fn blit_all(&mut self, src: PixmapRef) {
        if src.width() == self.pix.width() && src.height() == self.pix.height() {
            self.pix.data_mut().copy_from_slice(src.data());
        } else {
            self.pix.draw_pixmap(0, 0, src, &PixmapPaint::default(), Transform::identity(), None);
        }
    }

    /// Copies the device-pixel region of a logical rectangle from a same-sized pixmap.
    pub fn blit_region(&mut self, src: PixmapRef, r: R) {
        let (w, h) = (self.pix.width() as i32, self.pix.height() as i32);
        if src.width() as i32 != w || src.height() as i32 != h {
            return;
        }
        let s = self.scale;
        let x0 = ((r.x * s).floor() as i32).clamp(0, w);
        let y0 = ((r.y * s).floor() as i32).clamp(0, h);
        let x1 = ((r.right() * s).ceil() as i32).clamp(0, w);
        let y1 = ((r.bottom() * s).ceil() as i32).clamp(0, h);
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let stride = w as usize * 4;
        let (a, b) = (x0 as usize * 4, x1 as usize * 4);
        let sd = src.data();
        let dd = self.pix.data_mut();
        for y in y0 as usize..y1 as usize {
            let o = y * stride;
            dd[o + a..o + b].copy_from_slice(&sd[o + a..o + b]);
        }
    }

    // MARK: text

    /// Draws text with its baseline at `y`; `align` 0 left, 0.5 centre, 1 right. Returns the width.
    pub fn text(&mut self, s: &str, x: f32, y: f32, style: &Style, c: Rgba, align: f32) -> f32 {
        let fonts = self.fonts;
        let w = fonts.measure(s, style);
        let x0 = x - w * align;
        let col = self.color(c);
        let scale = self.scale;
        let clip = self.clip.take();
        fonts.draw(&mut self.pix, scale, s, x0, y, style, col, clip.as_ref());
        self.clip = clip;
        w
    }

    /// Centres text vertically in `r` (by cap height) with the given horizontal alignment.
    pub fn text_in(&mut self, s: &str, r: R, style: &Style, c: Rgba, align: f32) -> f32 {
        let cap = self.fonts.cap_height(style);
        let x = r.x + r.w * align;
        self.text(s, x, r.cy() + cap * 0.5, style, c, align)
    }
}
