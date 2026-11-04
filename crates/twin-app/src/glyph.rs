//! Rasterizes Twin's cast mark (the menu bar icon and the popover's logo)
//! and the popover shell (beak + gradient) from the SVG path geometry in
//! DESIGN.pen.

use kurbo::{BezPath, PathEl};
use tiny_skia::{
    Color, FillRule, GradientStop, LinearGradient, Mask, Paint, PathBuilder, Pixmap, PixmapPaint,
    Point, PremultipliedColorU8, Shader, SpreadMode, Stroke, Transform,
};

use crate::theme::Appearance;

/// Twin's cast mark, theSVG's Cast icon (thesvg.org/ui/cast, MIT) on its
/// 24-unit grid: a screen open at the bottom left, two waves rising into it.
/// Stroked, not filled, 2.4 units wide with round ends.
const CAST: [&str; 3] = [
    "M3 8L3 6.5A1.5 1.5 0 0 1 4.5 5L17 5A4 4 0 0 1 21 9L21 15.5A3.5 3.5 0 0 1 17.5 19L14 19",
    "M3 12.5C6.59 12.5 9.5 15.41 9.5 19",
    "M3 16.5C4.381 16.5 5.5 17.619 5.5 19",
];
const CAST_STROKE: f32 = 2.4;
/// The mark's ink with its stroke: [x, y, w, h] on the 24-unit grid
const CAST_BOUNDS: [f32; 4] = [1.8, 3.8, 20.4, 16.4];

fn stroke_cast(pixmap: &mut Pixmap, shader: Shader<'static>, width: f32, transform: Transform) {
    let paint = Paint {
        shader,
        anti_alias: true,
        ..Default::default()
    };
    let stroke = Stroke {
        width,
        line_cap: tiny_skia::LineCap::Round,
        line_join: tiny_skia::LineJoin::Round,
        ..Default::default()
    };
    for part in CAST {
        if let Some(path) = skia_path(part) {
            pixmap.stroke_path(&path, &paint, &stroke, transform, None);
        }
    }
}

/// Popover shell: a 380-wide rounded body with a beak pointing up at the menu
/// bar icon. Coordinates match the `Shell Shape` path in DESIGN.pen; the body
/// height follows the window height.
const SHELL_W: f32 = 380.0;

fn shell_path(height: f32) -> String {
    let side = height - 9.0 - 28.0;
    format!(
        "M14 9l161 0c5 0 8.6-1.2 11-4.8 1.2-1.8 2.2-2.7 4-2.7 1.8 0 2.8 0.9 4 2.7 2.4 3.6 6 4.8 11 4.8l161 0a14 14 0 0 1 14 14l0 {side}a14 14 0 0 1-14 14l-352 0a14 14 0 0 1-14-14l0-{side}a14 14 0 0 1 14-14z"
    )
}

pub struct Raster {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Menu bar icon: the glyph as a black template image, tightly fit to its
/// bounds and rendered taller than its slot so macOS downscales it into a crisp
/// white (dark mode) or black (light mode) status item. The disconnected state
/// uses narrower, fully opaque Wi-Fi arcs while the connected state retains the
/// original wave weight, making connection state readable without a faint icon.
pub fn menubar_icon(height: u32, connected: bool) -> Raster {
    let [gx, gy, gw, gh] = CAST_BOUNDS;
    let h = height as f32;
    let pad = h * 0.12;
    let scale = (h - 2.0 * pad) / gh;
    let width = (gw * scale + 2.0 * pad).ceil() as u32;

    // A phone connected draws the mark at its full weight; none, lighter
    let mut pixmap = Pixmap::new(width, height).expect("menu bar icon pixmap");
    let transform =
        Transform::from_scale(scale, scale).post_translate(pad - gx * scale, pad - gy * scale);
    let weight = if connected {
        CAST_STROKE
    } else {
        CAST_STROKE * 0.8
    };
    stroke_cast(
        &mut pixmap,
        Shader::SolidColor(Color::BLACK),
        weight,
        transform,
    );

    Raster {
        width,
        height,
        rgba: pixmap.take(),
    }
}

/// Preview of the disconnected (top) and connected (bottom) weights as macOS
/// template images on dark and light menu bars. For `twin-app --render-menubar`.
pub fn menubar_preview_png() -> Vec<u8> {
    let disconnected = menubar_icon(44, false);
    let connected = menubar_icon(44, true);
    let zoom = 6;
    let inset = 28;
    let cell_w = connected.width * zoom + inset * 2;
    let cell_h = connected.height * zoom + inset * 2;

    let mut pixmap = Pixmap::new(cell_w * 2, cell_h * 2).expect("preview pixmap");
    fill_cell(
        &mut pixmap,
        0.0,
        cell_w as f32,
        (cell_h * 2) as f32,
        "#1D1D1F",
    );
    fill_cell(
        &mut pixmap,
        cell_w as f32,
        cell_w as f32,
        (cell_h * 2) as f32,
        "#ECECEE",
    );

    let paint = PixmapPaint::default();
    let place = |dx: u32, dy: u32| {
        Transform::from_scale(zoom as f32, zoom as f32).post_translate(dx as f32, dy as f32)
    };
    for (row, icon) in [&disconnected, &connected].into_iter().enumerate() {
        let y = row as u32 * cell_h + inset;
        pixmap.draw_pixmap(
            0,
            0,
            recolor(icon, 0xFF).as_ref(),
            &paint,
            place(inset, y),
            None,
        );
        pixmap.draw_pixmap(
            0,
            0,
            recolor(icon, 0x00).as_ref(),
            &paint,
            place(cell_w + inset, y),
            None,
        );
    }

    pixmap.encode_png().expect("preview png")
}

fn recolor(icon: &Raster, level: u8) -> Pixmap {
    let mut pixmap = Pixmap::new(icon.width, icon.height).expect("recolor pixmap");
    for (pixel, chunk) in pixmap
        .pixels_mut()
        .iter_mut()
        .zip(icon.rgba.as_chunks::<4>().0)
    {
        let alpha = chunk[3];
        let value = ((u16::from(level) * u16::from(alpha)) / 255) as u8;
        *pixel = PremultipliedColorU8::from_rgba(value, value, value, alpha)
            .expect("valid premultiplied gray");
    }
    pixmap
}

fn fill_cell(pixmap: &mut Pixmap, x: f32, w: f32, h: f32, hex: &str) {
    let bytes = u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap_or(0);
    let color = Color::from_rgba8((bytes >> 16) as u8, (bytes >> 8) as u8, bytes as u8, 0xFF);
    let mut paint = Paint::default();
    paint.set_color(color);
    if let Some(rect) = tiny_skia::Rect::from_xywh(x, 0.0, w, h) {
        pixmap.fill_rect(rect, &paint, Transform::identity(), None);
    }
}

/// The popover's logo: the cast mark fading from the iPhone's blue to the
/// Android green, filling a `frame_size` square.
pub fn window_logo(frame_size: u32, oversample: u32) -> Raster {
    let px = frame_size * oversample;
    let mut pixmap = Pixmap::new(px, px).expect("logo pixmap");
    let scale = px as f32 / 24.0;
    let transform = Transform::from_scale(scale, scale);
    let gradient = LinearGradient::new(
        Point::from_xy(0.0, CAST_BOUNDS[1]),
        Point::from_xy(0.0, CAST_BOUNDS[1] + CAST_BOUNDS[3]),
        vec![
            GradientStop::new(0.0, Color::from_rgba8(0x7F, 0xD3, 0xFF, 0xFF)),
            GradientStop::new(1.0, Color::from_rgba8(0x3D, 0xDC, 0x84, 0xFF)),
        ],
        SpreadMode::Pad,
        Transform::identity(),
    )
    .expect("logo gradient");
    stroke_cast(&mut pixmap, gradient, CAST_STROKE, transform);

    Raster {
        width: px,
        height: px,
        rgba: pixmap.take(),
    }
}

struct ShellStyle {
    base: [Color; 3],
    /// Color the bottom edge is lit with, as an ellipse fading up the body.
    lift: Color,
    /// Soft bloom and hairline along the lit bottom edge.
    glow: Color,
    highlight: Color,
    border: Color,
}

fn shell_style(appearance: Appearance) -> ShellStyle {
    match appearance {
        Appearance::Night => ShellStyle {
            base: [
                Color::from_rgba8(0x2C, 0x2C, 0x2E, 0xFF),
                Color::from_rgba8(0x20, 0x20, 0x22, 0xFF),
                Color::from_rgba8(0x1C, 0x1C, 0x1E, 0xFF),
            ],
            lift: Color::from_rgba8(0xC8, 0xC8, 0xCC, 0x1C),
            glow: Color::from_rgba8(0xCC, 0xCC, 0xD0, 0x30),
            highlight: Color::from_rgba8(0xFF, 0xFF, 0xFF, 0x16),
            border: Color::from_rgba8(0xFF, 0xFF, 0xFF, 0x17),
        },
        Appearance::Day => ShellStyle {
            base: [
                Color::from_rgba8(0xFF, 0xFF, 0xFF, 0xFF),
                Color::from_rgba8(0xF7, 0xF8, 0xFB, 0xFF),
                Color::from_rgba8(0xEE, 0xF1, 0xF7, 0xFF),
            ],
            lift: Color::from_rgba8(0xC4, 0xD4, 0xF6, 0x58),
            glow: Color::from_rgba8(0x7E, 0x9C, 0xE6, 0x2E),
            highlight: Color::from_rgba8(0xFF, 0xFF, 0xFF, 0xD0),
            border: Color::from_rgba8(0x1B, 0x22, 0x3A, 0x1E),
        },
    }
}

fn transparent(color: Color) -> Color {
    let mut color = color;
    color.set_alpha(0.0);
    color
}

/// A horizontal line brightest in the middle and fading out toward both sides.
fn edge_line(color: Color) -> Shader<'static> {
    LinearGradient::new(
        Point::from_xy(0.0, 0.0),
        Point::from_xy(SHELL_W, 0.0),
        vec![
            GradientStop::new(0.08, transparent(color)),
            GradientStop::new(0.5, color),
            GradientStop::new(0.92, transparent(color)),
        ],
        SpreadMode::Pad,
        Transform::identity(),
    )
    .expect("shell edge gradient")
}

/// The top edge's highlight: bright on both sides of the beak and gone where
/// the beak meets the body (x 175 to 205), so the beak flows into the body
/// without a line across its base
fn top_line(color: Color) -> Shader<'static> {
    let beak = |x: f32| x / SHELL_W;
    LinearGradient::new(
        Point::from_xy(0.0, 0.0),
        Point::from_xy(SHELL_W, 0.0),
        vec![
            GradientStop::new(0.08, transparent(color)),
            GradientStop::new(beak(150.0), color),
            GradientStop::new(beak(172.0), transparent(color)),
            GradientStop::new(beak(208.0), transparent(color)),
            GradientStop::new(beak(230.0), color),
            GradientStop::new(0.92, transparent(color)),
        ],
        SpreadMode::Pad,
        Transform::identity(),
    )
    .expect("shell top gradient")
}

/// Rasterizes the popover shell (beak + rounded body): a vertical base
/// gradient lit from the bottom edge like the Agents Board cards, with a soft
/// bloom and a center-weighted hairline on that edge, an inner highlight along
/// the top and a hairline border. Drawn at `oversample` resolution.
fn render_shell(oversample: u32, appearance: Appearance, height: f32, gradients: bool) -> Pixmap {
    let w = (SHELL_W as u32) * oversample;
    let h = (height.round() as u32) * oversample;
    let mut pixmap = Pixmap::new(w, h).expect("shell pixmap");
    let transform = Transform::from_scale(oversample as f32, oversample as f32);

    let Some(path) = skia_path(&shell_path(height)) else {
        return pixmap;
    };
    let mut clip = Mask::new(w, h).expect("shell mask");
    clip.fill_path(&path, FillRule::Winding, true, transform);

    let style = shell_style(appearance);
    let body_top = 9.0;
    let mut paint = Paint {
        anti_alias: true,
        ..Default::default()
    };

    if !gradients {
        // Plain surface: one flat color under the hairline border.
        let full = tiny_skia::Rect::from_xywh(0.0, 0.0, SHELL_W, height).expect("shell rect");
        paint.set_color(style.base[1]);
        pixmap.fill_rect(full, &paint, transform, Some(&clip));
        stroke_border(&mut pixmap, &path, style.border, transform);
        return pixmap;
    }

    smooth_fill(&mut pixmap, &clip, &style, height, oversample as f32);
    let mut fill = |shader: Shader<'static>, rect: tiny_skia::Rect| {
        paint.shader = shader;
        pixmap.fill_rect(rect, &paint, transform, Some(&clip));
    };
    fill(
        edge_line(style.glow.with_alpha_scaled(2.4)),
        tiny_skia::Rect::from_xywh(0.0, height - 1.5, SHELL_W, 1.5).expect("glow line"),
    );
    fill(
        top_line(style.highlight),
        tiny_skia::Rect::from_xywh(0.0, body_top, SHELL_W, 1.2).expect("highlight line"),
    );

    stroke_border(&mut pixmap, &path, style.border, transform);
    pixmap
}

/// The base gradient, the light rising from the bottom edge and the lift
/// under the beak, computed in floating point and dithered once while
/// rounding to 8 bits: their shallow ramps span only a few levels, and plain
/// rounding shows each level as a band.
fn smooth_fill(pixmap: &mut Pixmap, clip: &Mask, style: &ShellStyle, height: f32, oversample: f32) {
    let width = pixmap.width() as usize;
    let center = SHELL_W / 2.0;
    let body_height = height - 9.0;
    let rgb = |color: Color| [color.red(), color.green(), color.blue()].map(|v| v * 255.0);
    let base = style.base.map(rgb);
    let glows = [
        (
            style.lift,
            center,
            height,
            SHELL_W * 0.8,
            body_height * 1.1,
            0.78,
        ),
        (style.glow, center, height, SHELL_W * 0.6, 34.0, 0.72),
        // A faint lift under the beak keeps the anchor point readable.
        (style.highlight, center, 9.0, 70.0, 18.0, 1.0),
    ];
    let coverage = clip.data();
    for (index, pixel) in pixmap
        .data_mut()
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .enumerate()
    {
        let cover = coverage[index];
        if cover == 0 {
            continue;
        }
        let x = ((index % width) as f32 + 0.5) / oversample;
        let y = ((index / width) as f32 + 0.5) / oversample;
        let t = (y / height).clamp(0.0, 1.0);
        let (from, to, mix) = if t < 0.5 {
            (base[0], base[1], t * 2.0)
        } else {
            (base[1], base[2], t * 2.0 - 1.0)
        };
        let mut color = [0, 1, 2].map(|c| from[c] + (to[c] - from[c]) * mix);
        for (glow, cx, cy, rx, ry, reach) in glows {
            let distance = (((x - cx) / rx).powi(2) + ((y - cy) / ry).powi(2)).sqrt();
            if distance < reach {
                let alpha = glow.alpha() * (1.0 - distance / reach);
                let glow = rgb(glow);
                for c in 0..3 {
                    color[c] += (glow[c] - color[c]) * alpha;
                }
            }
        }
        // Triangular noise of up to one level, the same on every channel so
        // it adds no color speckle
        let noise =
            noise_unit(index as u32, 0x68E3_1DA4) + noise_unit(index as u32, 0xB529_7A4D) - 1.0;
        let alpha = f32::from(cover) / 255.0;
        for c in 0..3 {
            pixel[c] = ((color[c] + noise).clamp(0.0, 255.0) * alpha).round() as u8;
        }
        pixel[3] = cover;
    }
}

/// A repeatable pseudo-random value in 0..1 for a pixel index and a seed
fn noise_unit(index: u32, seed: u32) -> f32 {
    let mut hash = index.wrapping_mul(0x9E37_79B1) ^ seed;
    hash ^= hash >> 16;
    hash = hash.wrapping_mul(0x7FEB_352D);
    hash ^= hash >> 15;
    hash = hash.wrapping_mul(0x846C_A68B);
    hash ^= hash >> 16;
    (hash >> 8) as f32 / (1 << 24) as f32
}

fn stroke_border(pixmap: &mut Pixmap, path: &tiny_skia::Path, color: Color, transform: Transform) {
    let mut paint = Paint {
        anti_alias: true,
        ..Default::default()
    };
    paint.set_color(color);
    let stroke = Stroke {
        width: 1.0,
        ..Default::default()
    };
    pixmap.stroke_path(path, &paint, &stroke, transform, None);
}

trait ScaleAlpha {
    fn with_alpha_scaled(self, factor: f32) -> Self;
}

impl ScaleAlpha for Color {
    fn with_alpha_scaled(mut self, factor: f32) -> Self {
        self.set_alpha((self.alpha() * factor).min(1.0));
        self
    }
}

/// The shell as a premultiplied-RGBA raster for the in-window texture.
pub fn shell_background(
    oversample: u32,
    appearance: Appearance,
    height: f32,
    gradients: bool,
) -> Raster {
    let pixmap = render_shell(oversample, appearance, height, gradients);
    Raster {
        width: pixmap.width(),
        height: pixmap.height(),
        rgba: pixmap.take(),
    }
}

/// The shell as a PNG, for offline preview via `twin-app --render-shell`.
pub fn shell_background_png(oversample: u32, appearance: Appearance, height: f32) -> Vec<u8> {
    render_shell(oversample, appearance, height, true)
        .encode_png()
        .expect("shell png encoding")
}

fn skia_path(svg_path: &str) -> Option<tiny_skia::Path> {
    let bez = BezPath::from_svg(svg_path).expect("valid design path");
    let mut builder = PathBuilder::new();

    for element in bez.elements() {
        match *element {
            PathEl::MoveTo(p) => builder.move_to(p.x as f32, p.y as f32),
            PathEl::LineTo(p) => builder.line_to(p.x as f32, p.y as f32),
            PathEl::QuadTo(c, p) => builder.quad_to(c.x as f32, c.y as f32, p.x as f32, p.y as f32),
            PathEl::CurveTo(c1, c2, p) => builder.cubic_to(
                c1.x as f32,
                c1.y as f32,
                c2.x as f32,
                c2.y as f32,
                p.x as f32,
                p.y as f32,
            ),
            PathEl::ClosePath => builder.close(),
        }
    }

    builder.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn day_and_night_shells_share_geometry_but_not_pixels() {
        let day = shell_background(1, Appearance::Day, 349.0, true);
        let night = shell_background(1, Appearance::Night, 349.0, true);

        assert_eq!((day.width, day.height), (380, 349));
        assert_eq!((night.width, night.height), (380, 349));
        assert_ne!(day.rgba, night.rgba);
    }

    #[test]
    fn disconnected_menu_icon_preserves_size_with_lighter_weight() {
        let disconnected = menubar_icon(44, false);
        let connected = menubar_icon(44, true);
        let alpha_coverage = |raster: &Raster| {
            raster
                .rgba
                .as_chunks::<4>()
                .0
                .iter()
                .map(|pixel| u64::from(pixel[3]))
                .sum::<u64>()
        };

        assert_eq!(disconnected.width, connected.width);
        assert_eq!(disconnected.height, connected.height);
        let disconnected_coverage = alpha_coverage(&disconnected);
        let connected_coverage = alpha_coverage(&connected);
        assert!(disconnected_coverage < connected_coverage);
        assert!(disconnected_coverage * 4 >= connected_coverage * 3);
    }
}
