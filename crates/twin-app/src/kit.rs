//! The redesign's building blocks: grouped cards, 36-point icon buttons,
//! toggles, the segmented control and the device icons, painted as vectors
//! on the 24-unit grid of the Twin redesign rather than taken from an icon
//! font.

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontFamily, FontId, Painter, Pos2, Rect, Response, Sense,
    Shape, Stroke, StrokeKind, Ui, pos2, vec2,
};

use crate::theme;

/// The popover's inner padding and the grid most gaps use
pub const PAD: f32 = 20.0;
pub const CARD_RADIUS: f32 = 14.0;
/// A card row's inset from the card's left edge; buttons sit closer to the right
pub const ROW_LEFT: f32 = 16.0;
pub const ROW_RIGHT: f32 = 12.0;
pub const BUTTON: f32 = 36.0;
pub const ICON: f32 = 20.0;
/// A list row in Virtual and in Settings
pub const LIST_ROW: f32 = 52.0;
pub const SETTING_ROW: f32 = 44.0;
const HOVER_TIME: f32 = 0.14;
const PRESS_TIME: f32 = 0.07;

pub fn font(size: f32, family: &str) -> FontId {
    FontId::new(size, FontFamily::Name(family.into()))
}

pub fn regular_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

/// Whether `response` holds focus that came from the keyboard (Tab), not a
/// click
pub fn keyboard_focused(ui: &Ui, response: &Response) -> bool {
    ui.memory(|memory| memory.has_focus(response.id))
        && ui
            .ctx()
            .data(|data| data.get_temp::<bool>(keyboard_focus_id()))
            .unwrap_or(false)
}

/// Whether focus last moved by keyboard, so focus rings show for Tab but
/// not after a click
pub fn keyboard_focus_id() -> egui::Id {
    egui::Id::new("twin-keyboard-focus")
}

/// The ring a control focused from the keyboard wears, so focus never reads
/// as selection
pub fn focus_ring(ui: &Ui, response: &Response, rect: Rect, radius: f32) {
    if keyboard_focused(ui, response) {
        ui.painter().rect_stroke(
            rect.expand(1.0),
            radius + 1.0,
            Stroke::new(1.5_f32, theme::input_focus_stroke()),
            StrokeKind::Outside,
        );
    }
}

/// Hover and press highlight, eased in and out
pub fn overlay(ui: &Ui, response: &Response) -> (Color32, f32) {
    let hover = ui.ctx().animate_bool_with_time(
        response.id.with("kit-hover"),
        response.hovered(),
        HOVER_TIME,
    );
    let press = ui.ctx().animate_bool_with_time(
        response.id.with("kit-press"),
        response.is_pointer_button_down_on(),
        PRESS_TIME,
    );
    (overlay_color(hover, press), hover)
}

pub fn overlay_color(hover_progress: f32, press_progress: f32) -> Color32 {
    theme::control_hover()
        .gamma_multiply(hover_progress)
        .blend(theme::control_pressed().gamma_multiply(press_progress))
}

/// A section heading over a card
pub fn heading(ui: &mut Ui, text: &str, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 15.0), Sense::hover());
    ui.painter().text(
        pos2(rect.left() + 4.0, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        font(12.0, theme::SEMIBOLD),
        color,
    );
    ui.add_space(8.0);
}

/// A card holding rows laid out top to bottom by `add`; its height follows
/// the rows
pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let background = ui.painter().add(Shape::Noop);
    let top = ui.cursor().top();
    let width = ui.available_width();
    let inner = ui
        .scope(|ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            ui.set_width(width);
            add(ui)
        })
        .inner;
    let rect = Rect::from_min_max(
        pos2(ui.min_rect().left(), top),
        pos2(ui.min_rect().left() + width, ui.cursor().top()),
    );
    ui.painter().set(
        background,
        Shape::rect_filled(rect, CARD_RADIUS, theme::card_fill()),
    );
    ui.painter().rect_stroke(
        rect,
        CARD_RADIUS,
        Stroke::new(1.0_f32, theme::card_stroke()),
        StrokeKind::Inside,
    );
    inner
}

/// A hairline between two rows of a card, edge to edge
pub fn row_divider(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter()
        .rect_filled(rect.shrink2(vec2(1.0, 0.0)), 0.0, theme::card_stroke());
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Glyph {
    Play,
    Stop,
    Bin,
    Wifi,
    Usb,
    Android,
    Show,
    Refresh,
    /// A window with a sidebar: Open as window
    Window,
    /// Into the corner of a window: back to the menu bar
    Back,
}

/// Paints `glyph` in `rect` (a 24-unit grid scaled to fit) with `color`
pub fn paint_glyph(painter: &Painter, rect: Rect, glyph: Glyph, color: Color32) {
    let unit = rect.width() / 24.0;
    let at = |x: f32, y: f32| pos2(rect.left() + x * unit, rect.top() + y * unit);
    let line = |width: f32| Stroke::new(width * unit, color);
    // Round caps and joins: a dot of the stroke's width at every vertex
    let polyline = |points: &[Pos2], width: f32| {
        painter.add(Shape::line(points.to_vec(), line(width)));
        for point in points {
            painter.circle_filled(*point, width * unit / 2.0, color);
        }
    };
    let arc = |cx: f32, cy: f32, r: f32, from: f32, to: f32| -> Vec<Pos2> {
        (0..=24)
            .map(|step| {
                let angle = from + (to - from) * step as f32 / 24.0;
                at(cx + r * angle.cos(), cy + r * angle.sin())
            })
            .collect()
    };
    match glyph {
        Glyph::Play => {
            let corners = [at(9.0, 7.2), at(9.0, 16.8), at(16.6, 12.0)];
            painter.add(Shape::convex_polygon(corners.to_vec(), color, Stroke::NONE));
            let mut closed = corners.to_vec();
            closed.push(corners[0]);
            polyline(&closed, 3.2);
        }
        Glyph::Stop => {
            painter.rect_filled(
                Rect::from_min_max(at(5.0, 5.0), at(19.0, 19.0)),
                3.0 * unit,
                color,
            );
        }
        Glyph::Bin => {
            polyline(&[at(6.5, 8.0), at(17.5, 8.0)], 2.0);
            polyline(&[at(10.5, 5.5), at(13.5, 5.5)], 2.0);
            polyline(
                &[
                    at(8.0, 8.0),
                    at(8.7, 17.4),
                    at(9.2, 18.4),
                    at(10.2, 18.8),
                    at(13.8, 18.8),
                    at(14.8, 18.4),
                    at(15.3, 17.4),
                    at(16.0, 8.0),
                ],
                2.0,
            );
        }
        Glyph::Wifi => {
            // Two arcs over a dot, centered on the dot's column
            let outer = 7.5_f32.atan2(18.55 - 10.5);
            polyline(
                &arc(
                    12.0,
                    18.55,
                    11.0,
                    -std::f32::consts::FRAC_PI_2 - outer,
                    -std::f32::consts::FRAC_PI_2 + outer,
                ),
                1.8,
            );
            let inner = 4.0_f32.atan2(18.47 - 14.0);
            polyline(
                &arc(
                    12.0,
                    18.47,
                    6.0,
                    -std::f32::consts::FRAC_PI_2 - inner,
                    -std::f32::consts::FRAC_PI_2 + inner,
                ),
                1.8,
            );
            painter.circle_filled(at(12.0, 17.6), 1.2 * unit, color);
        }
        Glyph::Usb => {
            // A USB-C plug seen side on: a pill with its contact line
            painter.rect_stroke(
                Rect::from_min_max(at(2.5, 8.0), at(21.5, 16.0)),
                4.0 * unit,
                line(1.8),
                StrokeKind::Middle,
            );
            polyline(&[at(8.0, 12.0), at(16.0, 12.0)], 1.8);
        }
        Glyph::Android => {
            let mut head = arc(
                12.0,
                17.0,
                8.0,
                std::f32::consts::PI,
                2.0 * std::f32::consts::PI,
            );
            head.push(at(4.0, 17.0));
            polyline(&head, 2.0);
            polyline(&[at(7.0, 7.0), at(9.0, 10.0)], 2.0);
            polyline(&[at(17.0, 7.0), at(15.0, 10.0)], 2.0);
        }
        Glyph::Refresh => {
            use std::f32::consts::PI;
            // Drawn at three quarters so its visual weight matches the
            // other glyphs, which fill about 13 of the 24 units
            let k = |x: f32, y: f32| at(12.0 + (x - 12.0) * 0.75, 12.0 + (y - 12.0) * 0.75);
            let mut top = arc(12.0, 12.0, 6.75, PI, 1.75 * PI);
            top.push(k(21.0, 8.0));
            polyline(&top, 2.0);
            let mut bottom = arc(12.0, 12.0, 6.75, 0.0, 0.75 * PI);
            bottom.push(k(3.0, 16.0));
            polyline(&bottom, 2.0);
            polyline(&[k(21.0, 3.0), k(21.0, 8.0), k(16.0, 8.0)], 2.0);
            polyline(&[k(3.0, 21.0), k(3.0, 16.0), k(8.0, 16.0)], 2.0);
        }
        Glyph::Window => {
            painter.rect_stroke(
                Rect::from_min_max(at(3.0, 4.5), at(21.0, 19.5)),
                2.5 * unit,
                line(2.0),
                StrokeKind::Middle,
            );
            polyline(&[at(9.0, 4.5), at(9.0, 19.5)], 2.0);
        }
        Glyph::Back => {
            polyline(&[at(10.0, 20.0), at(4.0, 20.0), at(4.0, 14.0)], 2.0);
            polyline(&[at(4.0, 20.0), at(12.0, 12.0)], 2.0);
            polyline(
                &[
                    at(6.0, 10.0),
                    at(6.0, 6.0),
                    at(6.6, 4.6),
                    at(8.0, 4.0),
                    at(18.0, 4.0),
                    at(19.4, 4.6),
                    at(20.0, 6.0),
                    at(20.0, 16.0),
                    at(19.4, 17.4),
                    at(18.0, 18.0),
                    at(14.0, 18.0),
                ],
                2.0,
            );
        }
        Glyph::Show => {
            // A window with an arrow out of its corner
            polyline(&[at(14.0, 4.0), at(20.0, 4.0), at(20.0, 10.0)], 2.0);
            polyline(&[at(20.0, 4.0), at(12.0, 12.0)], 2.0);
            polyline(
                &[
                    at(18.0, 14.0),
                    at(18.0, 18.0),
                    at(17.4, 19.4),
                    at(16.0, 20.0),
                    at(6.0, 20.0),
                    at(4.6, 19.4),
                    at(4.0, 18.0),
                    at(4.0, 8.0),
                    at(4.6, 6.6),
                    at(6.0, 6.0),
                    at(10.0, 6.0),
                ],
                2.0,
            );
        }
    }
}

/// What a square button shows and how
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ButtonLook {
    /// The plain gray button
    Normal,
    /// Green, for what is running: Stop
    Active,
    /// Shown faded and ignores clicks
    Disabled,
    /// A spinner in place of the glyph, for work in progress
    Busy,
}

pub enum Face {
    Glyph(Glyph),
}

/// A 36-point icon button
pub fn square_button(ui: &mut Ui, face: Face, look: ButtonLook, tooltip: &str) -> Response {
    let sense = if matches!(look, ButtonLook::Disabled | ButtonLook::Busy) {
        Sense::hover()
    } else {
        Sense::click()
    };
    let (rect, response) = ui.allocate_exact_size(vec2(BUTTON, BUTTON), sense);
    let (hover, _) = overlay(ui, &response);
    let painter = ui.painter();
    let (fill, ink) = match look {
        ButtonLook::Active => (theme::green_fill(), theme::green()),
        _ => (theme::button_fill(), theme::button_ink()),
    };
    let disabled = look == ButtonLook::Disabled;
    painter.rect_filled(
        rect,
        10.0,
        if disabled {
            fill.gamma_multiply(0.35)
        } else {
            fill
        },
    );
    if !matches!(look, ButtonLook::Disabled | ButtonLook::Busy) {
        painter.rect_filled(rect, 10.0, hover);
    }
    focus_ring(ui, &response, rect, 10.0);
    let icon_rect = Rect::from_center_size(rect.center(), vec2(ICON, ICON));
    if look == ButtonLook::Busy {
        egui::Spinner::new()
            .size(14.0)
            .color(theme::icon_muted())
            .paint_at(ui, Rect::from_center_size(rect.center(), vec2(14.0, 14.0)));
    } else {
        // An opaque dim ink: a translucent one would show darker dots where
        // the glyph's strokes overlap
        let ink = if disabled { theme::ink_disabled() } else { ink };
        let Face::Glyph(glyph) = face;
        paint_glyph(painter, icon_rect, glyph, ink);
    }
    let response = if look == ButtonLook::Disabled || look == ButtonLook::Busy {
        response
    } else {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    };
    if tooltip.is_empty() {
        response
    } else {
        response.on_hover_text(tooltip)
    }
}

/// A plain icon button with no fill until hovered: Refresh
pub fn ghost_button(ui: &mut Ui, glyph: Glyph, tooltip: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(BUTTON, BUTTON), Sense::click());
    let (hover, progress) = overlay(ui, &response);
    ui.painter().rect_filled(rect, 10.0, hover);
    focus_ring(ui, &response, rect, 10.0);
    paint_glyph(
        ui.painter(),
        Rect::from_center_size(rect.center(), vec2(ICON, ICON)),
        glyph,
        theme::tab_inactive().lerp_to_gamma(theme::text_strong(), progress),
    );
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(tooltip)
}

/// A text button with a leading icon and no fill until hovered: "Open as
/// window"
pub fn link_button(ui: &mut Ui, glyph: Glyph, label: &str) -> Response {
    let text_font = font(12.0, theme::MEDIUM);
    let width = ui
        .painter()
        .layout_no_wrap(label.to_owned(), text_font.clone(), Color32::WHITE)
        .size()
        .x;
    let size = vec2(8.0 + ICON + 8.0 + width + 8.0, BUTTON);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let (hover, progress) = overlay(ui, &response);
    let ink = theme::tab_inactive().lerp_to_gamma(theme::text_strong(), progress);
    focus_ring(ui, &response, rect, 10.0);
    let painter = ui.painter();
    painter.rect_filled(rect, 10.0, hover);
    paint_glyph(
        painter,
        Rect::from_center_size(
            pos2(rect.left() + 8.0 + ICON / 2.0, rect.center().y),
            vec2(ICON, ICON),
        ),
        glyph,
        ink,
    );
    painter.text(
        pos2(rect.left() + 8.0 + ICON + 8.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        text_font,
        ink,
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// An on/off switch; returns whether it was flipped
pub fn toggle(ui: &mut Ui, value: &mut bool, label: &str) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(32.0, 20.0), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    let mut changed = false;
    if response.clicked() {
        *value = !*value;
        changed = true;
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, *value, label)
    });
    let on = ui
        .ctx()
        .animate_bool_with_time(response.id.with("toggle"), *value, 0.16);
    let (hover, _) = overlay(ui, &response);
    let painter = ui.painter();
    painter.rect_filled(
        rect,
        10.0,
        theme::toggle_off().lerp_to_gamma(theme::green(), on),
    );
    painter.rect_filled(rect, 10.0, hover);
    // Off, the track keeps an edge on a light card
    painter.rect_stroke(
        rect,
        10.0,
        Stroke::new(1.0_f32, theme::field_stroke().gamma_multiply(1.0 - on)),
        StrokeKind::Inside,
    );
    focus_ring(ui, &response, rect, 10.0);
    let knob_x = egui::lerp(rect.left() + 10.0..=rect.right() - 10.0, on);
    painter.circle_filled(pos2(knob_x, rect.center().y), 8.0, Color32::WHITE);
    changed
}

/// A segmented control; `position` is the selected index, fractional while
/// the selection slides between two segments. Returns the clicked index.
pub fn segmented(
    ui: &mut Ui,
    rect: Rect,
    labels: &[&str],
    selected: usize,
    position: f32,
    size: f32,
) -> Option<usize> {
    let painter = ui.painter().clone();
    painter.rect_filled(rect, 10.0, theme::segment_track());
    let inner = rect.shrink(3.0);
    let gap = 4.0;
    let count = labels.len() as f32;
    let width = (inner.width() - gap * (count - 1.0)) / count;
    let segment = |index: f32| {
        Rect::from_min_size(
            pos2(inner.left() + index * (width + gap), inner.top()),
            vec2(width, inner.height()),
        )
    };
    let pill = segment(position);
    painter.rect_filled(pill, 7.0, theme::segment_on());
    let mut clicked = None;
    for (index, label) in labels.iter().enumerate() {
        let rect = segment(index as f32);
        // The gaps between segments take clicks too
        let response = ui.interact(
            rect.expand2(vec2(gap / 2.0, 3.0)),
            ui.id().with(("segment", *label)),
            Sense::click(),
        );
        let (hover, progress) = overlay(ui, &response);
        if index != selected {
            painter.rect_filled(rect, 7.0, hover);
        }
        focus_ring(ui, &response, rect, 7.0);
        // Text brightens as the pill passes under it
        let under = (1.0 - (position - index as f32).abs()).clamp(0.0, 1.0);
        let ink =
            theme::tab_inactive().lerp_to_gamma(theme::text_strong(), under.max(progress * 0.6));
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            *label,
            font(
                size,
                if under > 0.5 {
                    theme::SEMIBOLD
                } else {
                    theme::MEDIUM
                },
            ),
            ink,
        );
        if response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            clicked = Some(index);
        }
    }
    clicked
}

/// The rounded tile behind a phone's mark
pub fn paint_tile(painter: &Painter, rect: Rect, active: bool) {
    painter.rect_filled(
        rect,
        CornerRadius::same(11),
        if active {
            theme::green_tile()
        } else {
            theme::tile_fill()
        },
    );
}

/// Google's mark, theSVG's `google` mono icon: one path on a 24-unit grid
const GOOGLE_PATH: &str = "M12.48 10.92v3.28h7.84c-.24 1.84-.853 3.187-1.787 4.133-1.147 1.147-2.933 2.4-6.053 2.4-4.827 0-8.6-3.893-8.6-8.72s3.773-8.72 8.6-8.72c2.6 0 4.507 1.027 5.907 2.347l2.307-2.307C18.747 1.44 16.133 0 12.48 0 5.867 0 .307 5.387.307 12s5.56 12 12.173 12c3.573 0 6.267-1.173 8.373-3.36 2.16-2.16 2.84-5.213 2.84-7.667 0-.76-.053-1.467-.173-2.053H12.48z";

/// Paints Google's mark in `ink`, filling `rect`
pub fn paint_google(painter: &Painter, rect: Rect, ink: Color32) {
    let ctx = painter.ctx();
    let id = egui::Id::new("twin-google-mark");
    let texture = ctx
        .data(|data| data.get_temp::<egui::TextureHandle>(id))
        .unwrap_or_else(|| {
            let texture = ctx.load_texture(
                "google-mark",
                google_image(96),
                egui::TextureOptions::LINEAR,
            );
            ctx.data_mut(|data| data.insert_temp(id, texture.clone()));
            texture
        });
    painter.image(
        texture.id(),
        rect,
        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
        ink,
    );
}

/// The mark in white on transparent, `px` square, to tint when painted
fn google_image(px: u32) -> egui::ColorImage {
    let size = [px as usize, px as usize];
    let Some(mut pixmap) = tiny_skia::Pixmap::new(px, px) else {
        return egui::ColorImage::new(size, vec![Color32::TRANSPARENT; size[0] * size[1]]);
    };
    if let Some(path) = svg_path(GOOGLE_PATH) {
        let mut paint = tiny_skia::Paint::default();
        paint.set_color(tiny_skia::Color::WHITE);
        paint.anti_alias = true;
        let scale = px as f32 / 24.0;
        pixmap.fill_path(
            &path,
            &paint,
            tiny_skia::FillRule::Winding,
            tiny_skia::Transform::from_scale(scale, scale),
            None,
        );
    }
    egui::ColorImage::from_rgba_premultiplied(size, pixmap.data())
}

/// Reads an SVG path's M, L, H, V, C, S and Z commands, absolute or relative
fn svg_path(d: &str) -> Option<tiny_skia::Path> {
    let bytes = d.as_bytes();
    let mut i = 0;
    let skip = |i: &mut usize| {
        while *i < bytes.len() && (bytes[*i] == b' ' || bytes[*i] == b',') {
            *i += 1;
        }
    };
    let number = |i: &mut usize| -> Option<f32> {
        skip(i);
        let start = *i;
        if *i < bytes.len() && (bytes[*i] == b'-' || bytes[*i] == b'+') {
            *i += 1;
        }
        let mut dot = false;
        while *i < bytes.len() && (bytes[*i].is_ascii_digit() || (bytes[*i] == b'.' && !dot)) {
            dot |= bytes[*i] == b'.';
            *i += 1;
        }
        d.get(start..*i)?.parse().ok()
    };
    let mut path = tiny_skia::PathBuilder::new();
    let (mut x, mut y, mut start_x, mut start_y) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    let mut control: Option<(f32, f32)> = None;
    let mut command = b'M';
    loop {
        skip(&mut i);
        if i >= bytes.len() {
            break;
        }
        if bytes[i].is_ascii_alphabetic() {
            command = bytes[i];
            i += 1;
            if command.eq_ignore_ascii_case(&b'z') {
                path.close();
                (x, y) = (start_x, start_y);
                control = None;
                continue;
            }
        }
        let relative = command.is_ascii_lowercase();
        let (ox, oy) = if relative { (x, y) } else { (0.0, 0.0) };
        match command.to_ascii_uppercase() {
            b'M' => {
                (x, y) = (number(&mut i)? + ox, number(&mut i)? + oy);
                (start_x, start_y) = (x, y);
                path.move_to(x, y);
                // Further pairs after a move are lines
                command = if relative { b'l' } else { b'L' };
                control = None;
            }
            b'L' => {
                (x, y) = (number(&mut i)? + ox, number(&mut i)? + oy);
                path.line_to(x, y);
                control = None;
            }
            b'H' => {
                x = number(&mut i)? + ox;
                path.line_to(x, y);
                control = None;
            }
            b'V' => {
                y = number(&mut i)? + oy;
                path.line_to(x, y);
                control = None;
            }
            b'C' => {
                let (x1, y1) = (number(&mut i)? + ox, number(&mut i)? + oy);
                let (x2, y2) = (number(&mut i)? + ox, number(&mut i)? + oy);
                (x, y) = (number(&mut i)? + ox, number(&mut i)? + oy);
                path.cubic_to(x1, y1, x2, y2, x, y);
                control = Some((x2, y2));
            }
            b'S' => {
                let (x1, y1) = control.map_or((x, y), |(cx, cy)| (2.0 * x - cx, 2.0 * y - cy));
                let (x2, y2) = (number(&mut i)? + ox, number(&mut i)? + oy);
                (x, y) = (number(&mut i)? + ox, number(&mut i)? + oy);
                path.cubic_to(x1, y1, x2, y2, x, y);
                control = Some((x2, y2));
            }
            _ => return None,
        }
    }
    path.finish()
}

#[cfg(test)]
mod google_tests {
    use super::*;

    #[test]
    fn google_path_parses_and_fills_the_grid() {
        let path = svg_path(GOOGLE_PATH).expect("path");
        let bounds = path.bounds();
        assert!((bounds.left() - 0.307).abs() < 0.01, "{bounds:?}");
        assert!((bounds.top() - 0.0).abs() < 0.01, "{bounds:?}");
        assert!((bounds.right() - 23.85).abs() < 0.2, "{bounds:?}");
        assert!((bounds.bottom() - 24.0).abs() < 0.01, "{bounds:?}");
    }

    #[test]
    fn relative_numbers_without_separators() {
        let path = svg_path("M1 1h2v2H1z m5 5l1-.5.5.5z").expect("path");
        let bounds = path.bounds();
        assert_eq!((bounds.left(), bounds.top()), (1.0, 1.0));
        assert_eq!((bounds.right(), bounds.bottom()), (7.5, 6.0));
    }
}
