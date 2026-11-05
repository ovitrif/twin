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

/// Popover shell: a 420-wide body with 16-point corners and a beak pointing
/// up at the menu bar icon, as in the Twin redesign. The path runs half a
/// point inside the pixmap so the whole hairline border shows; the beak's
/// curve leaves the body's top edge and returns to it with no corner, so one
/// stroke draws both. The body height follows the window height.
const SHELL_W: f32 = 420.0;
const BODY_TOP: f32 = 12.0;

fn shell_path(height: f32) -> String {
    let (left, right, top, bottom) = (0.5, SHELL_W - 0.5, BODY_TOP, height - 0.5);
    let r = 15.5;
    let beak = SHELL_W / 2.0 - 20.0;
    format!(
        "M{l0} {top}L{beak} {top}C{b9} {top} {b13} {t2} {b16} {t7} {b17} {t9} {b18} {t11} {b20} {t11}C{b21} {t11} {b22} {t9} {b24} {t7} {b27} {t2} {b31} {top} {b40} {top}L{r0} {top}A{r} {r} 0 0 1 {right} {tr}L{right} {br}A{r} {r} 0 0 1 {r0} {bottom}L{l0} {bottom}A{r} {r} 0 0 1 {left} {br}L{left} {tr}A{r} {r} 0 0 1 {l0} {top}Z",
        l0 = left + r,
        r0 = right - r,
        tr = top + r,
        br = bottom - r,
        b9 = beak + 9.0,
        b13 = beak + 13.0,
        b16 = beak + 16.0,
        b17 = beak + 17.5,
        b18 = beak + 18.5,
        b20 = beak + 20.0,
        b21 = beak + 21.5,
        b22 = beak + 22.5,
        b24 = beak + 24.0,
        b27 = beak + 27.0,
        b31 = beak + 31.0,
        b40 = beak + 40.0,
        t2 = top - 2.0,
        t7 = top - 7.0,
        t9 = top - 9.5,
        t11 = top - 11.0,
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
    /// The surface's vertical gradient: top, at 60 % down, bottom
    base: [Color; 3],
    border: Color,
    /// The hairline glints lighting the border's middle, top and bottom
    top_glint: Color,
    bottom_glint: Color,
}

fn shell_style(appearance: Appearance) -> ShellStyle {
    match appearance {
        Appearance::Night => ShellStyle {
            base: [
                Color::from_rgba8(0x2A, 0x2A, 0x2C, 0xFF),
                Color::from_rgba8(0x1F, 0x1F, 0x21, 0xFF),
                Color::from_rgba8(0x1B, 0x1B, 0x1D, 0xFF),
            ],
            border: Color::from_rgba8(0xFF, 0xFF, 0xFF, 0x17),
            top_glint: Color::from_rgba8(0xFF, 0xFF, 0xFF, 0x57),
            bottom_glint: Color::from_rgba8(0xE1, 0xE6, 0xF5, 0xBF),
        },
        Appearance::Day => ShellStyle {
            base: [
                Color::from_rgba8(0xFF, 0xFF, 0xFF, 0xFF),
                Color::from_rgba8(0xF7, 0xF8, 0xFB, 0xFF),
                Color::from_rgba8(0xEF, 0xF1, 0xF6, 0xFF),
            ],
            border: Color::from_rgba8(0x1B, 0x22, 0x3A, 0x24),
            top_glint: Color::from_rgba8(0x1B, 0x22, 0x3A, 0x10),
            bottom_glint: Color::from_rgba8(0x6E, 0x8C, 0xD8, 0x8C),
        },
    }
}

fn transparent(color: Color) -> Color {
    let mut color = color;
    color.set_alpha(0.0);
    color
}

/// A horizontal ramp brightest in the middle, fading out by `edge` from
/// each side (a share of the width)
fn glint(color: Color, edge: f32) -> Shader<'static> {
    LinearGradient::new(
        Point::from_xy(0.0, 0.0),
        Point::from_xy(SHELL_W, 0.0),
        vec![
            GradientStop::new(edge, transparent(color)),
            GradientStop::new(0.5, color),
            GradientStop::new(1.0 - edge, transparent(color)),
        ],
        SpreadMode::Pad,
        Transform::identity(),
    )
    .expect("shell glint gradient")
}

/// Rasterizes the popover shell (beak + rounded body): the window's vertical
/// gradient, dithered, under a hairline border whose middle is lit along the
/// top (beak included) and the bottom. Drawn at `oversample` resolution.
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
    if !gradients {
        // Plain surface: one flat color under the hairline border.
        let mut paint = Paint {
            anti_alias: true,
            ..Default::default()
        };
        let full = tiny_skia::Rect::from_xywh(0.0, 0.0, SHELL_W, height).expect("shell rect");
        paint.set_color(style.base[1]);
        pixmap.fill_rect(full, &paint, transform, Some(&clip));
        stroke_border(&mut pixmap, &path, style.border.into(), transform, None);
        return pixmap;
    }

    smooth_fill(&mut pixmap, &clip, &style, height, oversample as f32);
    stroke_border(&mut pixmap, &path, style.border.into(), transform, None);
    // Each glint strokes the border again, kept to its edge by a band mask
    let band = |top: f32, bottom: f32| {
        let mut mask = Mask::new(w, h).expect("glint mask");
        let rect = tiny_skia::Rect::from_ltrb(0.0, top, SHELL_W, bottom).expect("glint band");
        mask.fill_path(
            &PathBuilder::from_rect(rect),
            FillRule::Winding,
            false,
            transform,
        );
        mask
    };
    let top = band(0.0, BODY_TOP + 1.0);
    stroke_border(
        &mut pixmap,
        &path,
        Border::Shader(glint(style.top_glint, 0.18)),
        transform,
        Some(&top),
    );
    let bottom = band(height - 1.5, height);
    stroke_border(
        &mut pixmap,
        &path,
        Border::Shader(glint(style.bottom_glint, 0.12)),
        transform,
        Some(&bottom),
    );
    pixmap
}

/// The base gradient, the light rising from the bottom edge and the lift
/// under the beak, computed in floating point and dithered once while
/// rounding to 8 bits: their shallow ramps span only a few levels, and plain
/// rounding shows each level as a band.
fn smooth_fill(pixmap: &mut Pixmap, clip: &Mask, style: &ShellStyle, height: f32, oversample: f32) {
    let width = pixmap.width() as usize;
    let rgb = |color: Color| [color.red(), color.green(), color.blue()].map(|v| v * 255.0);
    let base = style.base.map(rgb);
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
        let y = ((index / width) as f32 + 0.5) / oversample;
        let t = ((y - BODY_TOP) / (height - BODY_TOP)).clamp(0.0, 1.0);
        let (from, to, mix) = if t < 0.6 {
            (base[0], base[1], t / 0.6)
        } else {
            (base[1], base[2], (t - 0.6) / 0.4)
        };
        let color = [0, 1, 2].map(|c| from[c] + (to[c] - from[c]) * mix);
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

enum Border {
    Color(Color),
    Shader(Shader<'static>),
}

impl From<Color> for Border {
    fn from(color: Color) -> Self {
        Self::Color(color)
    }
}

fn stroke_border(
    pixmap: &mut Pixmap,
    path: &tiny_skia::Path,
    border: Border,
    transform: Transform,
    mask: Option<&Mask>,
) {
    let mut paint = Paint {
        anti_alias: true,
        ..Default::default()
    };
    match border {
        Border::Color(color) => paint.set_color(color),
        Border::Shader(shader) => paint.shader = shader,
    }
    let stroke = Stroke {
        width: 1.0,
        ..Default::default()
    };
    pixmap.stroke_path(path, &paint, &stroke, transform, mask);
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

        assert_eq!((day.width, day.height), (420, 349));
        assert_eq!((night.width, night.height), (420, 349));
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
