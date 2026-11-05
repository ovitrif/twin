//! Design tokens and font setup extracted from DESIGN.pen.

use std::sync::RwLock;

use eframe::egui::{
    Color32, Context, FontData, FontDefinitions, FontFamily, FontId, RichText, TextStyle, Theme,
};

pub const WINDOW_WIDTH: f32 = 420.0;
pub const WINDOW_HEIGHT: f32 = 600.0;
/// The beak rises above the rounded body and points at the menu bar icon.
pub const BEAK_HEIGHT: f32 = 12.0;
pub const WINDOW_FULL_HEIGHT: f32 = WINDOW_HEIGHT + BEAK_HEIGHT;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Appearance {
    Day,
    Night,
}

#[derive(Debug, Clone, Copy)]
struct Palette {
    text_strong: Color32,
    text_bright: Color32,
    text_soft: Color32,
    text_muted: Color32,
    text_faint: Color32,
    text_check: Color32,
    green: Color32,
    green_ink: Color32,
    red: Color32,
    surface: Color32,
    input_focus_stroke: Color32,
    control_hover: Color32,
    control_pressed: Color32,
    control_selected: Color32,
    scrollbar_thumb: Color32,
    scrollbar_thumb_hover: Color32,
    qr_card_stroke: Color32,
    qr_card: Color32,
    qr_ink: Color32,
    card_fill: Color32,
    card_stroke: Color32,
    button_fill: Color32,
    tile_fill: Color32,
    green_fill: Color32,
    green_tile: Color32,
    heading: Color32,
    button_ink: Color32,
    icon_muted: Color32,
    tab_inactive: Color32,
    segment_track: Color32,
    segment_on: Color32,
    toggle_off: Color32,
    field_fill: Color32,
    field_stroke: Color32,
    sidebar_fill: Color32,
    nav_on: Color32,
}

impl Palette {
    fn mix(from: Self, to: Self, progress: f32) -> Self {
        let mix = |from, to| mix_color(from, to, progress);
        Self {
            text_strong: mix(from.text_strong, to.text_strong),
            text_bright: mix(from.text_bright, to.text_bright),
            text_soft: mix(from.text_soft, to.text_soft),
            text_muted: mix(from.text_muted, to.text_muted),
            text_faint: mix(from.text_faint, to.text_faint),
            text_check: mix(from.text_check, to.text_check),
            green: mix(from.green, to.green),
            green_ink: mix(from.green_ink, to.green_ink),
            red: mix(from.red, to.red),
            surface: mix(from.surface, to.surface),
            input_focus_stroke: mix(from.input_focus_stroke, to.input_focus_stroke),
            control_hover: mix(from.control_hover, to.control_hover),
            control_pressed: mix(from.control_pressed, to.control_pressed),
            control_selected: mix(from.control_selected, to.control_selected),
            scrollbar_thumb: mix(from.scrollbar_thumb, to.scrollbar_thumb),
            scrollbar_thumb_hover: mix(from.scrollbar_thumb_hover, to.scrollbar_thumb_hover),
            qr_card_stroke: mix(from.qr_card_stroke, to.qr_card_stroke),
            qr_card: mix(from.qr_card, to.qr_card),
            qr_ink: mix(from.qr_ink, to.qr_ink),
            card_fill: mix(from.card_fill, to.card_fill),
            card_stroke: mix(from.card_stroke, to.card_stroke),
            button_fill: mix(from.button_fill, to.button_fill),
            tile_fill: mix(from.tile_fill, to.tile_fill),
            green_fill: mix(from.green_fill, to.green_fill),
            green_tile: mix(from.green_tile, to.green_tile),
            heading: mix(from.heading, to.heading),
            button_ink: mix(from.button_ink, to.button_ink),
            icon_muted: mix(from.icon_muted, to.icon_muted),
            tab_inactive: mix(from.tab_inactive, to.tab_inactive),
            segment_track: mix(from.segment_track, to.segment_track),
            segment_on: mix(from.segment_on, to.segment_on),
            toggle_off: mix(from.toggle_off, to.toggle_off),
            field_fill: mix(from.field_fill, to.field_fill),
            field_stroke: mix(from.field_stroke, to.field_stroke),
            sidebar_fill: mix(from.sidebar_fill, to.sidebar_fill),
            nav_on: mix(from.nav_on, to.nav_on),
        }
    }
}

fn mix_color(from: Color32, to: Color32, progress: f32) -> Color32 {
    let progress = progress.clamp(0.0, 1.0);
    let channel = |from: u8, to: u8| {
        (f32::from(from) + (f32::from(to) - f32::from(from)) * progress).round() as u8
    };
    Color32::from_rgba_premultiplied(
        channel(from.r(), to.r()),
        channel(from.g(), to.g()),
        channel(from.b(), to.b()),
        channel(from.a(), to.a()),
    )
}

const NIGHT: Palette = Palette {
    text_strong: Color32::from_rgb(0xF2, 0xF3, 0xF7),
    text_bright: Color32::from_rgb(0xEC, 0xEE, 0xF4),
    text_soft: Color32::from_rgb(0x83, 0x87, 0x91),
    text_muted: Color32::from_rgb(0x90, 0x94, 0xA0),
    text_faint: Color32::from_rgb(0x70, 0x74, 0x7F),
    text_check: Color32::from_rgb(0xC9, 0xCC, 0xD6),
    green: Color32::from_rgb(0x3D, 0xDC, 0x84),
    green_ink: Color32::from_rgb(0x0A, 0x2A, 0x1B),
    red: Color32::from_rgb(0xF8, 0x71, 0x71),
    surface: Color32::from_rgba_premultiplied(0x0D, 0x0D, 0x0D, 0x0D),
    input_focus_stroke: Color32::from_rgb(0x4D, 0x9F, 0xF5),
    control_hover: Color32::from_rgba_premultiplied(0x18, 0x18, 0x18, 0x18),
    control_pressed: Color32::from_rgba_premultiplied(0x25, 0x25, 0x25, 0x25),
    control_selected: Color32::from_rgba_premultiplied(0x15, 0x15, 0x15, 0x15),
    scrollbar_thumb: Color32::from_rgba_premultiplied(0x4A, 0x4C, 0x52, 0x60),
    scrollbar_thumb_hover: Color32::from_rgba_premultiplied(0x8C, 0x8F, 0x99, 0xA0),
    qr_card_stroke: Color32::TRANSPARENT,
    qr_card: Color32::WHITE,
    qr_ink: Color32::from_rgb(0x17, 0x18, 0x1C),
    card_fill: Color32::from_rgba_premultiplied(0x0B, 0x0B, 0x0B, 0x0B),
    card_stroke: Color32::from_rgba_premultiplied(0x0F, 0x0F, 0x0F, 0x0F),
    button_fill: Color32::from_rgba_premultiplied(0x12, 0x12, 0x12, 0x12),
    tile_fill: Color32::from_rgba_premultiplied(0x0F, 0x0F, 0x0F, 0x0F),
    green_fill: Color32::from_rgba_premultiplied(0x0A, 0x23, 0x15, 0x29),
    green_tile: Color32::from_rgba_premultiplied(0x09, 0x1F, 0x12, 0x24),
    heading: Color32::from_rgb(0x8E, 0x91, 0x99),
    button_ink: Color32::from_rgb(0xE6, 0xE7, 0xEA),
    icon_muted: Color32::from_rgb(0x8E, 0x91, 0x99),
    tab_inactive: Color32::from_rgb(0x9F, 0xA2, 0xAA),
    segment_track: Color32::from_rgba_premultiplied(0x00, 0x00, 0x00, 0x40),
    segment_on: Color32::from_rgba_premultiplied(0x1A, 0x1A, 0x1A, 0x1A),
    toggle_off: Color32::from_rgba_premultiplied(0x29, 0x29, 0x29, 0x29),
    field_fill: Color32::from_rgba_premultiplied(0x00, 0x00, 0x00, 0x47),
    field_stroke: Color32::from_rgba_premultiplied(0x14, 0x14, 0x14, 0x14),
    sidebar_fill: Color32::from_rgba_premultiplied(0x00, 0x00, 0x00, 0x38),
    nav_on: Color32::from_rgba_premultiplied(0x14, 0x14, 0x14, 0x14),
};

const DAY: Palette = Palette {
    text_strong: Color32::from_rgb(0x17, 0x19, 0x23),
    text_bright: Color32::from_rgb(0x22, 0x24, 0x2E),
    text_soft: Color32::from_rgb(0x62, 0x68, 0x74),
    text_muted: Color32::from_rgb(0x54, 0x5B, 0x68),
    text_faint: Color32::from_rgb(0x75, 0x7D, 0x8A),
    text_check: Color32::from_rgb(0x2F, 0x33, 0x3D),
    green: Color32::from_rgb(0x14, 0x84, 0x47),
    green_ink: Color32::WHITE,
    red: Color32::from_rgb(0xD6, 0x3A, 0x3A),
    surface: Color32::from_rgba_premultiplied(0x00, 0x00, 0x00, 0x08),
    input_focus_stroke: Color32::from_rgb(0x3E, 0x8F, 0xE8),
    control_hover: Color32::from_rgba_premultiplied(0x00, 0x00, 0x00, 0x0C),
    control_pressed: Color32::from_rgba_premultiplied(0x00, 0x00, 0x00, 0x18),
    control_selected: Color32::from_rgba_premultiplied(0x00, 0x00, 0x00, 0x0F),
    scrollbar_thumb: Color32::from_rgba_premultiplied(0x1A, 0x1D, 0x24, 0x48),
    scrollbar_thumb_hover: Color32::from_rgba_premultiplied(0x2A, 0x2E, 0x38, 0x80),
    qr_card_stroke: Color32::from_rgba_premultiplied(0x00, 0x00, 0x00, 0x14),
    qr_card: Color32::WHITE,
    qr_ink: Color32::from_rgb(0x17, 0x18, 0x1C),
    card_fill: Color32::from_rgba_premultiplied(0xD9, 0xD9, 0xD9, 0xD9),
    card_stroke: Color32::from_rgba_premultiplied(0x00, 0x00, 0x00, 0x14),
    button_fill: Color32::from_rgba_premultiplied(0x00, 0x00, 0x00, 0x0D),
    tile_fill: Color32::from_rgba_premultiplied(0x00, 0x00, 0x00, 0x0B),
    green_fill: Color32::from_rgba_premultiplied(0x02, 0x10, 0x09, 0x1F),
    green_tile: Color32::from_rgba_premultiplied(0x02, 0x0E, 0x08, 0x1A),
    heading: Color32::from_rgb(0x6B, 0x71, 0x7D),
    button_ink: Color32::from_rgb(0x2A, 0x2D, 0x36),
    icon_muted: Color32::from_rgb(0x7B, 0x81, 0x8C),
    tab_inactive: Color32::from_rgb(0x5F, 0x66, 0x72),
    segment_track: Color32::from_rgba_premultiplied(0x00, 0x00, 0x00, 0x0F),
    segment_on: Color32::WHITE,
    toggle_off: Color32::from_rgba_premultiplied(0x00, 0x00, 0x00, 0x26),
    field_fill: Color32::WHITE,
    field_stroke: Color32::from_rgba_premultiplied(0x00, 0x00, 0x00, 0x1C),
    sidebar_fill: Color32::from_rgba_premultiplied(0x00, 0x00, 0x00, 0x08),
    nav_on: Color32::from_rgba_premultiplied(0x00, 0x00, 0x00, 0x0F),
};

static ACTIVE_PALETTE: RwLock<Palette> = RwLock::new(NIGHT);

pub fn apply(ctx: &Context, appearance: Appearance) {
    set_day_weight(match appearance {
        Appearance::Day => 1.0,
        Appearance::Night => 0.0,
    });
    ctx.set_theme(match appearance {
        Appearance::Day => Theme::Light,
        Appearance::Night => Theme::Dark,
    });
}

pub fn set_day_weight(day_weight: f32) {
    *ACTIVE_PALETTE
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Palette::mix(NIGHT, DAY, day_weight);
}

fn palette() -> Palette {
    *ACTIVE_PALETTE
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

macro_rules! color_token {
    ($name:ident, $field:ident) => {
        pub fn $name() -> Color32 {
            palette().$field
        }
    };
}

color_token!(text_strong, text_strong);
color_token!(text_bright, text_bright);
color_token!(text_soft, text_soft);
color_token!(text_muted, text_muted);
color_token!(text_faint, text_faint);
color_token!(text_check, text_check);
color_token!(green, green);
color_token!(green_ink, green_ink);
color_token!(red, red);
color_token!(surface, surface);
color_token!(input_focus_stroke, input_focus_stroke);
color_token!(control_hover, control_hover);
color_token!(control_pressed, control_pressed);
color_token!(control_selected, control_selected);
color_token!(scrollbar_thumb, scrollbar_thumb);
color_token!(scrollbar_thumb_hover, scrollbar_thumb_hover);
color_token!(qr_card_stroke, qr_card_stroke);
color_token!(qr_card, qr_card);
color_token!(qr_ink, qr_ink);
color_token!(card_fill, card_fill);
color_token!(card_stroke, card_stroke);
color_token!(button_fill, button_fill);
color_token!(tile_fill, tile_fill);
color_token!(green_fill, green_fill);
color_token!(green_tile, green_tile);
color_token!(heading, heading);
color_token!(button_ink, button_ink);
color_token!(icon_muted, icon_muted);
color_token!(tab_inactive, tab_inactive);
color_token!(segment_track, segment_track);
color_token!(segment_on, segment_on);
color_token!(toggle_off, toggle_off);
color_token!(field_fill, field_fill);
color_token!(field_stroke, field_stroke);
color_token!(sidebar_fill, sidebar_fill);
color_token!(nav_on, nav_on);

pub const MEDIUM: &str = "inter-medium";
pub const SEMIBOLD: &str = "inter-semibold";
pub const BOLD: &str = "inter-bold";
pub const ICONS: &str = "icons";

pub fn install_fonts(ctx: &Context) {
    let mut fonts = FontDefinitions::default();

    let faces: [(&str, &[u8]); 4] = [
        ("inter", include_bytes!("../assets/fonts/Inter-Regular.ttf")),
        (MEDIUM, include_bytes!("../assets/fonts/Inter-Medium.ttf")),
        (
            SEMIBOLD,
            include_bytes!("../assets/fonts/Inter-SemiBold.ttf"),
        ),
        (BOLD, include_bytes!("../assets/fonts/Inter-Bold.ttf")),
    ];

    for (name, bytes) in faces {
        fonts
            .font_data
            .insert(name.to_owned(), FontData::from_static(bytes).into());
    }

    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);

    // Icons get a dedicated family. Phosphor cannot sit in a text family's
    // fallback chain (it blanks lowercase Latin), and Inter cannot precede
    // it for icons (Inter defines private-use glyphs that shadow icons).
    let proportional = fonts.families.get_mut(&FontFamily::Proportional).unwrap();
    proportional.retain(|name| name != "phosphor");
    proportional.insert(0, "inter".to_owned());

    fonts
        .families
        .insert(FontFamily::Name(ICONS.into()), vec!["phosphor".to_owned()]);

    for name in [MEDIUM, SEMIBOLD, BOLD] {
        fonts.families.insert(
            FontFamily::Name(name.into()),
            vec![name.to_owned(), "inter".to_owned()],
        );
    }

    ctx.set_fonts(fonts);

    ctx.all_styles_mut(|style| {
        style
            .text_styles
            .insert(TextStyle::Body, FontId::new(12.5, FontFamily::Proportional));
        style.visuals.override_text_color = None;
        style.spacing.item_spacing = eframe::egui::vec2(0.0, 0.0);
        style.spacing.button_padding = eframe::egui::vec2(0.0, 0.0);
        // Tooltips float on a soft, close shadow rather than egui's heavy one
        style.visuals.popup_shadow = eframe::egui::Shadow {
            offset: [0, 2],
            blur: 8,
            spread: 0,
            color: Color32::from_black_alpha(0x38),
        };
    });
}

pub fn regular(text: impl Into<String>, size: f32, color: Color32) -> RichText {
    RichText::new(text).size(size).color(color)
}

pub fn semibold(text: impl Into<String>, size: f32, color: Color32) -> RichText {
    RichText::new(text)
        .size(size)
        .color(color)
        .family(FontFamily::Name(SEMIBOLD.into()))
}

pub fn icon(glyph: &str, size: f32, color: Color32) -> RichText {
    RichText::new(glyph)
        .size(size)
        .color(color)
        .family(FontFamily::Name(ICONS.into()))
}

pub fn icon_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(ICONS.into()))
}
