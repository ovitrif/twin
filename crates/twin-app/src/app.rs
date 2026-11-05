use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui::containers::scroll_area::ScrollBarVisibility;
use eframe::egui::{
    self, Align, Color32, Context, FontFamily, FontId, Frame, Label, Layout, Margin, Rect, Sense,
    Stroke, TextEdit, TextureHandle, TextureOptions, Theme, Ui, ViewportCommand, vec2,
};
use egui_phosphor::regular as ph;
use menu_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use menu_icon::{
    Icon, MenuBarIcon, MenuBarIconBuilder, MenuBarIconEvent, MouseButton, MouseButtonState,
};

use crate::backend::{self, PairingPhase, PairingProgress, Shared};
use crate::config::{Settings, ThemeMode};
use crate::glyph;
use crate::kit;
use crate::login_item;
use crate::theme::{self, icon, regular, semibold};

const FOCUS_GRACE: Duration = Duration::from_millis(300);
const STATUS_POLL: Duration = Duration::from_secs(5);
const SCREEN_TRANSITION_DURATION: Duration = Duration::from_millis(280);
const SKIN_TRANSITION_DURATION: Duration = Duration::from_millis(180);
const POPOVER_APPEAR_DURATION: Duration = Duration::from_millis(160);
const POPOVER_HIDE_DURATION: Duration = Duration::from_millis(120);
const SCROLL_EDGE_FADE_HEIGHT: f32 = 22.0;
/// How far scroll viewports reach into the side margins: room for hover
/// shapes and focus rings at the content edge, and for the scrollbar.
const SCROLL_BLEED: f32 = 9.0;
const SCROLLBAR_MIN_HANDLE: f32 = 28.0;
const SCROLLBAR_WIDTH: f32 = 3.0;
const SCROLLBAR_HOVER_WIDTH: f32 = 6.0;
const SCROLLBAR_HIT_WIDTH: f32 = 9.0;
const SCROLLBAR_TRACK_INSET: f32 = 4.0;
/// Seconds the scrollbar stays after scrolling stops, then its fade length.
const SCROLLBAR_LINGER: f64 = 0.8;
const SCROLLBAR_FADE: f64 = 0.25;
const SHELL_OVERSAMPLE: u32 = 3;
/// How far pages may draw above the body, into the gap under the header.
const SCREEN_TOP_BLEED: f32 = 6.0;
const QR_CARD_SIZE: f32 = 164.0;
/// How long the Pair page shows a successful pairing before going back.
const PAIRED_HOLD: Duration = Duration::from_millis(1100);
/// How long after launch to keep forcing the window hidden, in case the
/// platform surfaces it despite `with_visible(false)`.
const STARTUP_HIDE: Duration = Duration::from_millis(800);

static STATUS_EVENTS: Mutex<Vec<MenuBarIconEvent>> = Mutex::new(Vec::new());
static MENU_EVENTS: Mutex<Vec<MenuEvent>> = Mutex::new(Vec::new());
static STATUS_PRIMARY_CLICK: AtomicBool = AtomicBool::new(false);

const POPOVER_GAP: f64 = 6.5;
const WINDOW_MARGIN: f64 = 8.0;

#[derive(Debug, Clone, Copy, PartialEq)]
struct DisplayBounds {
    min_x: f64,
    max_x: f64,
    min_y: f64,
    max_y: f64,
    scale: f64,
}

impl DisplayBounds {
    fn contains(&self, x: f64, y: f64) -> bool {
        (self.min_x..=self.max_x).contains(&x) && (self.min_y..=self.max_y).contains(&y)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct MenuAnchor {
    rect_x: f64,
    rect_y: f64,
    rect_width: f64,
    rect_height: f64,
    click_x: f64,
    click_y: f64,
}

impl MenuAnchor {
    fn new(rect: menu_icon::Rect, click_x: f64, click_y: f64) -> Self {
        Self {
            rect_x: rect.position.x,
            rect_y: rect.position.y,
            rect_width: f64::from(rect.size.width),
            rect_height: f64::from(rect.size.height),
            click_x,
            click_y,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct LogicalMenuAnchor {
    x: f64,
    bottom_y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Screen {
    Devices,
    Pair,
    Logs,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ScreenTransition {
    from: Screen,
    to: Screen,
    direction: f32,
    started_at: Instant,
    cancel_pairing_on_complete: bool,
    /// Where the selection was when the slide began: mid-way through an
    /// earlier slide, it carries on from there instead of jumping
    from_position: f32,
}

impl ScreenTransition {
    fn progress(&self) -> f32 {
        (self.started_at.elapsed().as_secs_f32() / SCREEN_TRANSITION_DURATION.as_secs_f32())
            .clamp(0.0, 1.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PopoverTransitionPhase {
    Appearing,
    Disappearing,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct PopoverTransition {
    phase: PopoverTransitionPhase,
    started_at: Instant,
}

#[derive(Debug, Clone, Copy)]
struct SkinTransition {
    from_day_weight: f32,
    target: theme::Appearance,
    started_at: Instant,
}

#[derive(Debug, Clone, Copy)]
struct SkinState {
    appearance: theme::Appearance,
    day_weight: f32,
    transition: Option<SkinTransition>,
}

#[derive(Debug, Clone, Copy)]
struct SkinUpdate {
    day_weight: f32,
    committed: Option<theme::Appearance>,
    animating: bool,
}

#[derive(Debug, Clone, Copy)]
struct AnimationState {
    screen: Option<ScreenTransition>,
    popover: Option<PopoverTransition>,
    skin: SkinState,
}

impl SkinState {
    fn new(appearance: theme::Appearance) -> Self {
        Self {
            appearance,
            day_weight: appearance_day_weight(appearance),
            transition: None,
        }
    }

    fn set_target_at(&mut self, target: theme::Appearance, now: Instant) -> bool {
        if self
            .transition
            .is_some_and(|active| active.target == target)
            || (self.transition.is_none() && self.appearance == target)
        {
            return false;
        }

        let from_day_weight = self.day_weight_at(now);
        self.day_weight = from_day_weight;
        self.transition = Some(SkinTransition {
            from_day_weight,
            target,
            started_at: now,
        });
        true
    }

    fn day_weight_at(&self, now: Instant) -> f32 {
        self.transition.map_or(self.day_weight, |transition| {
            skin_day_weight(transition, skin_transition_progress(transition, now))
        })
    }
}

impl AnimationState {
    fn new(appearance: theme::Appearance) -> Self {
        Self {
            screen: None,
            popover: None,
            skin: SkinState::new(appearance),
        }
    }

    fn advance_skin_at(&mut self, now: Instant) -> SkinUpdate {
        let Some(transition) = self.skin.transition else {
            return SkinUpdate {
                day_weight: self.skin.day_weight,
                committed: None,
                animating: false,
            };
        };

        let elapsed = now.saturating_duration_since(transition.started_at);
        if elapsed >= SKIN_TRANSITION_DURATION {
            let target = transition.target;
            let committed = self.commit_skin(target);
            return SkinUpdate {
                day_weight: self.skin.day_weight,
                committed: committed.then_some(target),
                animating: false,
            };
        }

        let progress = elapsed.as_secs_f32() / SKIN_TRANSITION_DURATION.as_secs_f32();
        self.skin.day_weight = skin_day_weight(transition, progress);
        SkinUpdate {
            day_weight: self.skin.day_weight,
            committed: None,
            animating: true,
        }
    }

    fn commit_skin(&mut self, target: theme::Appearance) -> bool {
        let changed = self.skin.appearance != target
            || self.skin.transition.is_some()
            || (self.skin.day_weight - appearance_day_weight(target)).abs() > f32::EPSILON;
        self.skin.appearance = target;
        self.skin.day_weight = appearance_day_weight(target);
        self.skin.transition = None;
        changed
    }
}

fn appearance_day_weight(appearance: theme::Appearance) -> f32 {
    match appearance {
        theme::Appearance::Day => 1.0,
        theme::Appearance::Night => 0.0,
    }
}

fn skin_transition_progress(transition: SkinTransition, now: Instant) -> f32 {
    now.saturating_duration_since(transition.started_at)
        .as_secs_f32()
        / SKIN_TRANSITION_DURATION.as_secs_f32()
}

fn skin_day_weight(transition: SkinTransition, progress: f32) -> f32 {
    let eased = egui::emath::easing::cubic_in_out(progress.clamp(0.0, 1.0));
    let target = appearance_day_weight(transition.target);
    transition.from_day_weight + (target - transition.from_day_weight) * eased
}

pub struct App {
    shared: Arc<Mutex<Shared>>,
    settings: Settings,
    width_text: String,
    height_text: String,
    gap_text: String,
    screen: Screen,
    animations: AnimationState,
    logo: TextureHandle,
    day_shell: TextureHandle,
    night_shell: TextureHandle,
    /// The menu bar status item; absent in the headless drive mode.
    menu_bar: Option<MenuBar>,
    /// Headless drive mode: no window or status item, always shown.
    headless: bool,
    _menu: Menu,
    menu_icon_connected: bool,
    show_item: MenuItem,
    pair_id: MenuId,
    refresh_id: MenuId,
    quit_id: MenuId,
    visible: bool,
    shown_at: Instant,
    focus_hidden_at: Option<Instant>,
    last_poll: Instant,
    created_at: Instant,
    open_at_login: Option<bool>,
    login_query: Option<std::sync::mpsc::Receiver<bool>>,
    pending_show: bool,
    pending_egui_theme: Option<theme::Appearance>,
    last_menu_anchor: Option<MenuAnchor>,
    auto_mirrored: HashSet<String>,
    pending_avd_delete: Option<String>,
    shell: Arc<ShellSampler>,
    pending_page_step: Option<i8>,
    /// Shown as a normal window instead of the menu bar popover
    detached: bool,
    /// Last run ended as a window: open it again once the app is up
    reopen_detached: bool,
    /// A window mode change for the next frame, which has the native window
    pending_mode: Option<bool>,
    /// Open the popover once the window has turned back into it
    pending_attach_show: bool,
    quitting: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> anyhow::Result<Self> {
        Self::create(cc, false)
    }

    /// The app without a window or menu bar item, shown from the start, for
    /// the headless drive mode.
    #[cfg(feature = "drive")]
    pub fn new_headless(cc: &eframe::CreationContext<'_>) -> anyhow::Result<Self> {
        Self::create(cc, true)
    }

    fn create(cc: &eframe::CreationContext<'_>, headless: bool) -> anyhow::Result<Self> {
        let ctx = cc.egui_ctx.clone();
        theme::install_fonts(&ctx);
        let settings = Settings::load();
        let appearance = resolved_appearance(settings.theme, ctx.system_theme());
        theme::apply(&ctx, appearance);

        let logo_raster = glyph::window_logo(26, 4);
        let logo_image = egui::ColorImage::from_rgba_premultiplied(
            [logo_raster.width as usize, logo_raster.height as usize],
            &logo_raster.rgba,
        );
        let logo = ctx.load_texture("twin-logo", logo_image, TextureOptions::LINEAR);

        let (day_shell, night_shell, shell) = load_shells(&ctx, settings.gradients);

        let menu = Menu::new();
        let show_item = MenuItem::new("Show Twin", true, None);
        let pair_item = MenuItem::new("Pair new device", true, None);
        let refresh_item = MenuItem::new("Refresh status", true, None);
        let quit_item = MenuItem::new("Quit Twin", true, None);
        menu.append_items(&[
            &show_item,
            &PredefinedMenuItem::separator(),
            &pair_item,
            &refresh_item,
            &PredefinedMenuItem::separator(),
            &quit_item,
        ])?;

        let menu_bar = if headless {
            None
        } else {
            Some(MenuBar::install(&ctx, &menu)?)
        };

        let status_ctx = ctx.clone();
        MenuBarIconEvent::set_event_handler(Some(move |event| {
            STATUS_EVENTS.lock().unwrap().push(event);
            status_ctx.request_repaint();
        }));

        let menu_ctx = ctx.clone();
        MenuEvent::set_event_handler(Some(move |event| {
            MENU_EVENTS.lock().unwrap().push(event);
            menu_ctx.request_repaint();
        }));

        let shared = Arc::new(Mutex::new(Shared::default()));
        if needs_full_status_refresh(false, settings.auto_mirror) {
            backend::refresh_status(shared.clone(), ctx.clone());
        } else {
            backend::refresh_connection_state(shared.clone(), ctx.clone());
        }

        let settings_detached = settings.detached && !headless;
        let width_text = settings.window_width.to_string();
        let height_text = settings.window_height.to_string();
        let gap_text = settings.gap.to_string();
        Ok(Self {
            shared,
            settings,
            width_text,
            height_text,
            gap_text,
            screen: Screen::Devices,
            animations: AnimationState::new(appearance),
            logo,
            day_shell,
            night_shell,
            menu_bar,
            headless,
            _menu: menu,
            menu_icon_connected: false,
            show_item,
            pair_id: pair_item.id().clone(),
            refresh_id: refresh_item.id().clone(),
            quit_id: quit_item.id().clone(),
            visible: headless,
            shown_at: Instant::now(),
            focus_hidden_at: None,
            last_poll: Instant::now(),
            created_at: Instant::now(),
            open_at_login: None,
            login_query: None,
            pending_show: false,
            pending_egui_theme: None,
            last_menu_anchor: None,
            auto_mirrored: HashSet::new(),
            pending_avd_delete: None,
            shell,
            pending_page_step: None,
            detached: false,
            reopen_detached: settings_detached,
            pending_mode: None,
            pending_attach_show: false,
            quitting: false,
        })
    }

    fn show(&mut self, ctx: &Context, anchor: Option<MenuAnchor>) {
        if let Some(anchor) = anchor {
            let fallback_monitor_width = ctx.input(|i| {
                i.viewport()
                    .monitor_size
                    .map(|monitor| f64::from(monitor.x))
            });
            let displays = active_display_bounds();
            let (x, y) = popover_position(anchor, fallback_monitor_width, &displays);

            ctx.send_viewport_cmd(ViewportCommand::OuterPosition([x as f32, y as f32].into()));
            // Reveal on the next frame so the move lands first; a freshly
            // created window would otherwise flash at its default centered
            // position on the very first open.
            self.pending_show = true;
            ctx.request_repaint();
        } else {
            ctx.send_viewport_cmd(ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(ViewportCommand::Focus);
        }

        self.visible = true;
        self.animations.popover = Some(PopoverTransition {
            phase: PopoverTransitionPhase::Appearing,
            started_at: Instant::now(),
        });
        self.shown_at = Instant::now();
        self.focus_hidden_at = None;
        self.show_item.set_text("Hide Twin");
        backend::refresh_status(self.shared.clone(), ctx.clone());
    }

    fn hide(&mut self, ctx: &Context) {
        if !self.visible || self.is_disappearing() {
            return;
        }

        self.pending_show = false;
        self.animations.popover = Some(PopoverTransition {
            phase: PopoverTransitionPhase::Disappearing,
            started_at: Instant::now(),
        });
        self.show_item.set_text("Show Twin");
        ctx.request_repaint();
    }

    fn toggle(&mut self, ctx: &Context, anchor: Option<MenuAnchor>) {
        if self.visible && !self.is_disappearing() {
            self.hide(ctx);
        } else if self
            .focus_hidden_at
            .is_some_and(|at| at.elapsed() < FOCUS_GRACE)
        {
            self.focus_hidden_at = None;
        } else {
            self.show(ctx, anchor);
        }
    }

    fn quit(&mut self, ctx: &Context) {
        self.quitting = true;
        backend::cancel_pairing(&self.shared);
        backend::stop_all_mirrors(&self.shared);
        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(ViewportCommand::Close);
    }

    /// Moves to `screen` with a slide: Pair starts pairing on entry and
    /// cancels it once the slide away from it completes.
    fn go(&mut self, screen: Screen, ctx: &Context) {
        if self.screen == screen {
            return;
        }
        // A delete dialog belongs to the device list; leaving it dismisses it.
        self.pending_avd_delete = None;
        // A transition replaced mid-slide would drop its pending cancel and
        // leave the pairing session running; cancel it now instead.
        if self
            .animations
            .screen
            .is_some_and(|transition| transition.cancel_pairing_on_complete)
        {
            backend::cancel_pairing(&self.shared);
        }
        let from_position = self.page_position();
        self.animations.screen = Some(ScreenTransition {
            from: self.screen,
            to: screen,
            direction: screen_transition_direction(self.screen, screen),
            started_at: Instant::now(),
            cancel_pairing_on_complete: self.screen == Screen::Pair,
            from_position,
        });
        self.screen = screen;
        if screen == Screen::Pair {
            backend::start_pairing(self.shared.clone(), ctx.clone());
        }
        ctx.request_repaint();
    }

    /// ← and →: the previous or next page, without wrapping
    fn step_page(&mut self, step: isize, ctx: &Context) {
        if let Some(screen) = screen_in_order(self.screen, step) {
            self.go(screen, ctx);
        }
    }

    /// Where the selection sits in the page order, between two pages while
    /// sliding, for the segmented control and the sidebar
    fn page_position(&self) -> f32 {
        match self.animations.screen {
            Some(transition) => egui::lerp(
                transition.from_position..=screen_index(transition.to) as f32,
                egui::emath::easing::cubic_in_out(transition.progress()),
            ),
            None => screen_index(self.screen) as f32,
        }
    }

    /// Opens Twin as a normal window; the popover hides until it closes
    fn detach(&mut self, ctx: &Context) {
        self.detached = true;
        self.settings.detached = true;
        self.save_settings();
        if !self.headless {
            self.visible = false;
            self.animations.popover = None;
            self.pending_show = false;
            self.pending_mode = Some(true);
            self.show_item.set_text("Show Twin");
            set_dock_presence(true);
        }
        ctx.request_repaint();
    }

    /// Back to the menu bar: closes the window, then opens the popover when
    /// `show` asks
    fn attach(&mut self, ctx: &Context, show: bool) {
        self.detached = false;
        self.settings.detached = false;
        self.save_settings();
        if !self.headless {
            self.pending_mode = Some(false);
            set_dock_presence(false);
            if show {
                self.pending_attach_show = true;
            }
        }
        ctx.request_repaint();
    }

    fn focus_window(&self, ctx: &Context) {
        set_dock_presence(true);
        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
    }

    /// Turns the one native window into the detached window or back into
    /// the popover, once `detach` or `attach` asked for it
    fn apply_window_mode(&mut self, ctx: &Context, frame: &eframe::Frame) {
        let Some(detached) = self.pending_mode.take() else {
            return;
        };
        set_native_window_mode(frame, detached);
        if detached {
            let size = WINDOW_SIZE;
            let monitor = ctx.input(|input| input.viewport().monitor_size);
            ctx.send_viewport_cmd(ViewportCommand::WindowLevel(egui::WindowLevel::Normal));
            ctx.send_viewport_cmd(ViewportCommand::MinInnerSize(WINDOW_MIN_SIZE));
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(size));
            if let Some(monitor) = monitor {
                ctx.send_viewport_cmd(ViewportCommand::OuterPosition(egui::pos2(
                    ((monitor.x - size.x) / 2.0).max(0.0),
                    ((monitor.y - size.y) / 3.0).max(24.0),
                )));
            }
            set_native_window_opacity(frame, 1.0);
            ctx.send_viewport_cmd(ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(ViewportCommand::Focus);
        } else {
            ctx.send_viewport_cmd(ViewportCommand::Visible(false));
            ctx.send_viewport_cmd(ViewportCommand::WindowLevel(egui::WindowLevel::AlwaysOnTop));
            ctx.send_viewport_cmd(ViewportCommand::MinInnerSize(egui::Vec2::ZERO));
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(vec2(
                theme::WINDOW_WIDTH,
                theme::WINDOW_FULL_HEIGHT,
            )));
        }
    }

    fn handle_events(&mut self, ctx: &Context) {
        let status_events: Vec<MenuBarIconEvent> =
            std::mem::take(&mut *STATUS_EVENTS.lock().unwrap());
        for event in status_events {
            if let MenuBarIconEvent::Click {
                button,
                button_state,
                rect,
                position,
                ..
            } = event
            {
                let anchor = MenuAnchor::new(rect, position.x, position.y);
                self.last_menu_anchor = Some(anchor);

                if button == MouseButton::Left && button_state == MouseButtonState::Up {
                    if self.detached {
                        self.focus_window(ctx);
                    } else {
                        self.toggle(ctx, Some(anchor));
                    }
                }
            }
        }

        if STATUS_PRIMARY_CLICK.swap(false, Ordering::SeqCst) {
            self.last_menu_anchor = None;
            let anchor = self.menu_anchor();
            self.last_menu_anchor = anchor;
            if self.detached {
                self.focus_window(ctx);
            } else {
                self.toggle(ctx, anchor);
            }
        }

        let menu_events: Vec<MenuEvent> = std::mem::take(&mut *MENU_EVENTS.lock().unwrap());
        for event in menu_events {
            if event.id == self.show_item.id() && self.detached {
                self.focus_window(ctx);
            } else if event.id == self.show_item.id() {
                if self.visible && !self.is_disappearing() {
                    self.hide(ctx);
                } else {
                    self.show(ctx, self.menu_anchor());
                }
            } else if event.id == self.pair_id {
                // Already on Pair (say, with an expired or failed code): start a
                // fresh pairing rather than reopening the old one.
                if self.screen == Screen::Pair {
                    backend::start_pairing(self.shared.clone(), ctx.clone());
                } else {
                    self.go(Screen::Pair, ctx);
                }
                if self.detached {
                    self.focus_window(ctx);
                } else {
                    self.show(ctx, self.menu_anchor());
                }
            } else if event.id == self.refresh_id {
                backend::refresh_status(self.shared.clone(), ctx.clone());
            } else if event.id == self.quit_id {
                self.quit(ctx);
            }
        }
    }

    fn handle_focus(&mut self, ctx: &Context) {
        if !self.visible || self.is_disappearing() || self.detached {
            return;
        }

        let focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
        if !focused && !self.headless && self.shown_at.elapsed() > FOCUS_GRACE {
            self.hide(ctx);
            self.focus_hidden_at = Some(Instant::now());
        }
    }

    fn handle_escape(&mut self, ctx: &Context) {
        if !self.visible || self.is_disappearing() {
            return;
        }

        let escape =
            ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        if !escape {
            return;
        }
        // Escape closes the innermost layer: an open delete dialog first,
        // then the popover.
        if self.pending_avd_delete.take().is_some() {
            ctx.request_repaint();
        } else {
            self.hide(ctx);
        }
    }

    fn handle_keyboard_navigation(&mut self, ctx: &Context) {
        let (tab, pointer) = ctx.input(|input| {
            (
                input.key_pressed(egui::Key::Tab),
                input.pointer.any_pressed(),
            )
        });
        if tab || pointer {
            ctx.data_mut(|data| data.insert_temp(keyboard_focus_id(), tab));
        }

        match self.pending_page_step.take() {
            Some(step) if (self.visible && !self.is_disappearing()) || self.detached => {
                self.step_page(isize::from(step), ctx);
            }
            _ => {}
        }
    }

    fn menu_anchor(&self) -> Option<MenuAnchor> {
        self.last_menu_anchor.or_else(|| {
            self.menu_bar.as_ref()?.icon.rect().map(|rect| {
                let click_x = rect.position.x + f64::from(rect.size.width) / 2.0;
                let click_y = rect.position.y + f64::from(rect.size.height) / 2.0;
                MenuAnchor::new(rect, click_x, click_y)
            })
        })
    }

    fn is_disappearing(&self) -> bool {
        self.animations
            .popover
            .is_some_and(|transition| transition.phase == PopoverTransitionPhase::Disappearing)
    }

    fn update_popover_transition(&mut self, ctx: &Context) {
        let Some(transition) = self.animations.popover else {
            return;
        };
        let duration = popover_transition_duration(transition.phase);

        if transition.started_at.elapsed() >= duration {
            self.animations.popover = None;
            if transition.phase == PopoverTransitionPhase::Disappearing {
                ctx.send_viewport_cmd(ViewportCommand::Visible(false));
                self.visible = false;
            }
        } else {
            ctx.request_repaint();
        }
    }

    fn save_settings(&mut self) {
        self.settings.window_width = self.width_text.trim().parse().unwrap_or(0);
        self.settings.window_height = self.height_text.trim().parse().unwrap_or(0);
        self.settings.gap = self.gap_text.trim().parse().unwrap_or(12);
        self.settings.save();
    }

    fn sync_appearance(&mut self, ctx: &Context, now: Instant) {
        let appearance = resolved_appearance(self.settings.theme, ctx.system_theme());
        if self.animations.skin.set_target_at(appearance, now) {
            ctx.request_repaint();
        }
    }

    fn update_skin_transition(&mut self, ctx: &Context, now: Instant) {
        let update = self.animations.advance_skin_at(now);
        theme::set_day_weight(update.day_weight);
        if let Some(appearance) = update.committed {
            self.pending_egui_theme = Some(appearance);
        }
        // Switching egui's theme changes its text anti-aliasing, which rebuilds
        // the font atlas. eframe drops texture uploads while the window is
        // hidden, so a rebuild then left the next popover drawing text from a
        // stale atlas. Apply it only on a frame that is actually painted.
        let painted = ctx
            .input(|input| input.viewport().visible())
            .unwrap_or(true);
        if (self.visible || self.detached)
            && painted
            && let Some(appearance) = self.pending_egui_theme.take()
        {
            theme::apply(ctx, appearance);
        }
        if update.animating {
            ctx.request_repaint();
        }
    }

    /// Auto-start mirroring for newly connected physical phones (never
    /// emulators) when the setting is on and scrcpy is available.
    fn maybe_auto_mirror(&mut self, ctx: &Context) {
        if !self.settings.auto_mirror {
            self.auto_mirrored.clear();
            return;
        }

        let (snapshot, mirroring) = {
            let state = self.shared.lock().unwrap();
            let mut mirroring = state.mirrors.keys().cloned().collect::<HashSet<String>>();
            mirroring.extend(state.starting_mirrors.iter().cloned());
            (state.snapshot.clone(), mirroring)
        };
        let Some(snapshot) = snapshot else { return };
        if !snapshot.mirror.available {
            return;
        }

        // Forget devices that have disconnected so a later reconnect re-mirrors.
        let present = ready_device_mirror_keys(&snapshot.devices);
        self.auto_mirrored
            .retain(|serial| present.contains(serial.as_str()));

        for device in &snapshot.devices {
            if device.ready
                && !device.is_emulator
                && !mirroring.contains(&device.mirror_key)
                && self.auto_mirrored.insert(device.mirror_key.clone())
            {
                backend::start_mirror(
                    self.shared.clone(),
                    ctx.clone(),
                    device.clone(),
                    self.settings.scrcpy_options(),
                    self.settings.pair_options(),
                );
            }
        }
    }

    fn sync_menu_icon(&mut self) {
        let connected = self.shared.lock().unwrap().device_connected;
        if connected == self.menu_icon_connected {
            return;
        }

        // `set_icon` clears the template flag on macOS, leaving a fixed black
        // glyph after the connection state changes. Keep every replacement a
        // template so AppKit recolors it when the menu-bar appearance changes.
        let Some(menu_bar) = &self.menu_bar else {
            return;
        };
        match menu_bar_icon(connected).and_then(|icon| {
            menu_bar
                .icon
                .set_icon_with_as_template(Some(icon), true)
                .map_err(Into::into)
        }) {
            Ok(()) => self.menu_icon_connected = connected,
            Err(error) => {
                self.shared
                    .lock()
                    .unwrap()
                    .log(format!("Menu bar icon update failed: {error:#}"));
            }
        }
    }
}

/// Hooks for the headless drive mode (see `drive.rs`).
#[cfg(feature = "drive")]
impl App {
    /// One line describing what is on screen, for drive replies.
    pub fn drive_state(&self) -> String {
        let pairing = self
            .shared
            .lock()
            .unwrap()
            .pairing
            .as_ref()
            .map_or("none", |session| match session.phase {
                PairingPhase::Qr { .. } => "qr",
                PairingPhase::Connecting { .. } => "connecting",
                PairingPhase::Failed { .. } => "failed",
                PairingPhase::Paired { .. } => "paired",
            });
        let transition = self
            .animations
            .screen
            .map_or(String::from("none"), |transition| {
                format!("{:?}->{:?}", transition.from, transition.to)
            });
        format!(
            "screen={:?} window={} transition={transition} theme={:?} gradients={} pairing={pairing}",
            self.screen, self.detached, self.settings.theme, self.settings.gradients,
        )
    }

    /// The drive window's size: the popover's, or the detached window's
    pub fn drive_size(&self) -> egui::Vec2 {
        if self.detached {
            WINDOW_SIZE
        } else {
            vec2(theme::WINDOW_WIDTH, theme::WINDOW_FULL_HEIGHT)
        }
    }

    pub fn drive_set_window(&mut self, ctx: &Context, on: bool) {
        if on {
            self.detach(ctx);
        } else {
            self.attach(ctx, false);
        }
    }

    pub fn drive_set_theme(&mut self, mode: ThemeMode) {
        self.settings.theme = mode;
    }

    pub fn drive_set_gradients(&mut self, ctx: &Context, on: bool) {
        self.settings.gradients = on;
        (self.day_shell, self.night_shell, self.shell) = load_shells(ctx, on);
    }
}

/// The menu bar status item and, on macOS, the target routing its clicks.
struct MenuBar {
    icon: MenuBarIcon,
    #[cfg(target_os = "macos")]
    _click: objc2::rc::Retained<crate::status_click::StatusClickTarget>,
}

impl MenuBar {
    fn install(ctx: &Context, menu: &Menu) -> anyhow::Result<Self> {
        let icon = menu_bar_icon(false)?;
        let builder = MenuBarIconBuilder::new()
            .with_icon(icon)
            .with_icon_as_template(true)
            .with_tooltip("Twin");
        #[cfg(not(target_os = "macos"))]
        let builder = builder
            .with_menu(Box::new(menu.clone()))
            .with_menu_on_left_click(false);
        let status_icon = builder.build()?;

        #[cfg(target_os = "macos")]
        let click = {
            use menu_icon::menu::ContextMenu;
            let primary_ctx = ctx.clone();
            let ns_menu = unsafe {
                objc2::rc::Retained::retain(menu.ns_menu().cast::<objc2_app_kit::NSMenu>())
            };
            status_icon
                .ns_status_item()
                .zip(ns_menu)
                .and_then(|(item, ns_menu)| {
                    crate::status_click::StatusClickTarget::install(item, ns_menu, move || {
                        STATUS_PRIMARY_CLICK.store(true, Ordering::SeqCst);
                        primary_ctx.request_repaint();
                    })
                })
                .ok_or_else(|| anyhow::anyhow!("menu bar status item unavailable"))?
        };
        #[cfg(not(target_os = "macos"))]
        let _ = ctx;

        Ok(Self {
            icon: status_icon,
            #[cfg(target_os = "macos")]
            _click: click,
        })
    }
}

fn ready_device_mirror_keys(devices: &[backend::DeviceInfo]) -> HashSet<&str> {
    devices
        .iter()
        .filter(|device| device.ready)
        .map(|device| device.mirror_key.as_str())
        .collect()
}

fn menu_bar_icon(connected: bool) -> anyhow::Result<Icon> {
    let raster = glyph::menubar_icon(44, connected);
    Ok(Icon::from_rgba(raster.rgba, raster.width, raster.height)?)
}

fn needs_full_status_refresh(visible: bool, auto_mirror: bool) -> bool {
    visible || auto_mirror
}

/// Day and night shell textures plus the sampler over both rasters.
fn load_shells(
    ctx: &Context,
    gradients: bool,
) -> (TextureHandle, TextureHandle, Arc<ShellSampler>) {
    let (day, day_raster) =
        load_shell_texture(ctx, "twin-shell-day", theme::Appearance::Day, gradients);
    let (night, night_raster) =
        load_shell_texture(ctx, "twin-shell-night", theme::Appearance::Night, gradients);
    (
        day,
        night,
        Arc::new(ShellSampler::new(night_raster, day_raster)),
    )
}

fn load_shell_texture(
    ctx: &Context,
    name: &str,
    appearance: theme::Appearance,
    gradients: bool,
) -> (TextureHandle, glyph::Raster) {
    let raster = glyph::shell_background(
        SHELL_OVERSAMPLE,
        appearance,
        theme::WINDOW_FULL_HEIGHT,
        gradients,
    );
    let image = egui::ColorImage::from_rgba_premultiplied(
        [raster.width as usize, raster.height as usize],
        &raster.rgba,
    );
    (
        ctx.load_texture(name, image, TextureOptions::LINEAR),
        raster,
    )
}

/// Both shell rasters, kept so scroll edges can fade into the exact surface
/// color beneath them instead of painting a flat band over the lit gradient.
struct ShellSampler {
    width: u32,
    height: u32,
    night: Vec<u8>,
    day: Vec<u8>,
}

impl ShellSampler {
    fn new(night: glyph::Raster, day: glyph::Raster) -> Self {
        Self {
            width: night.width,
            height: night.height,
            night: night.rgba,
            day: day.rgba,
        }
    }

    fn color_at(&self, pos: egui::Pos2, day_weight: f32) -> Color32 {
        let scale = SHELL_OVERSAMPLE as f32;
        let x = ((pos.x * scale).max(0.0) as u32).min(self.width - 1);
        let y = ((pos.y * scale).max(0.0) as u32).min(self.height - 1);
        let index = ((y * self.width + x) * 4) as usize;
        let pick = |rgba: &[u8]| {
            Color32::from_rgba_premultiplied(
                rgba[index],
                rgba[index + 1],
                rgba[index + 2],
                rgba[index + 3],
            )
        };
        pick(&self.night).lerp_to_gamma(pick(&self.day), day_weight)
    }
}

fn resolved_appearance(mode: ThemeMode, system_theme: Option<Theme>) -> theme::Appearance {
    match mode {
        ThemeMode::Day => theme::Appearance::Day,
        ThemeMode::Night => theme::Appearance::Night,
        ThemeMode::Auto => match system_theme {
            Some(Theme::Light) => theme::Appearance::Day,
            Some(Theme::Dark) | None => theme::Appearance::Night,
        },
    }
}

const SCREEN_ORDER: [Screen; 4] = [
    Screen::Devices,
    Screen::Pair,
    Screen::Logs,
    Screen::Settings,
];

fn screen_index(screen: Screen) -> usize {
    SCREEN_ORDER
        .iter()
        .position(|candidate| *candidate == screen)
        .unwrap_or(0)
}

const TITLE_HEIGHT: f32 = 32.0;
const TABS_HEIGHT: f32 = 34.0;
/// The detached window's opening size and the smallest it resizes to
const WINDOW_SIZE: egui::Vec2 = vec2(860.0, 600.0);
const WINDOW_MIN_SIZE: egui::Vec2 = vec2(700.0, 480.0);
const SIDEBAR_WIDTH: f32 = 208.0;
const NAV_ITEM: f32 = 36.0;
const PHONE_ROW: f32 = 40.0;
const WINDOW_PAGE_WIDTH: f32 = 640.0;

fn tab_label(screen: Screen) -> &'static str {
    match screen {
        Screen::Devices => "Devices",
        Screen::Pair => "Pair",
        Screen::Logs => "Logs",
        Screen::Settings => "Settings",
    }
}

fn window_title(screen: Screen) -> &'static str {
    match screen {
        Screen::Pair => "Pair a phone",
        screen => tab_label(screen),
    }
}

/// The window's surface at `t` (0 at the top, 1 at the bottom): the
/// popover's gradient, or its middle color with gradients off
fn window_surface_color(t: f32, day_weight: f32, gradients: bool) -> Color32 {
    let night = [
        Color32::from_rgb(0x2A, 0x2A, 0x2C),
        Color32::from_rgb(0x1F, 0x1F, 0x21),
        Color32::from_rgb(0x1B, 0x1B, 0x1D),
    ];
    let day = [
        Color32::from_rgb(0xFF, 0xFF, 0xFF),
        Color32::from_rgb(0xF7, 0xF8, 0xFB),
        Color32::from_rgb(0xEF, 0xF1, 0xF6),
    ];
    let at = |stops: [Color32; 3]| {
        if !gradients {
            stops[1]
        } else if t < 0.6 {
            stops[0].lerp_to_gamma(stops[1], (t / 0.6).clamp(0.0, 1.0))
        } else {
            stops[1].lerp_to_gamma(stops[2], ((t - 0.6) / 0.4).clamp(0.0, 1.0))
        }
    };
    at(night).lerp_to_gamma(at(day), day_weight)
}

/// The window's surface with the glints along its top and bottom edges
fn paint_window_surface(painter: &egui::Painter, rect: Rect, day_weight: f32, gradients: bool) {
    let mut mesh = egui::Mesh::default();
    for t in [0.0, 0.3, 0.6, 0.8, 1.0] {
        let color = window_surface_color(t, day_weight, gradients);
        let y = egui::lerp(rect.top()..=rect.bottom(), t);
        mesh.colored_vertex(egui::pos2(rect.left(), y), color);
        mesh.colored_vertex(egui::pos2(rect.right(), y), color);
    }
    for band in 0..4 {
        let index = band * 2;
        mesh.add_triangle(index, index + 1, index + 2);
        mesh.add_triangle(index + 1, index + 3, index + 2);
    }
    painter.add(egui::Shape::mesh(mesh));
    if !gradients {
        return;
    }
    let glint = |y: f32, edge: f32, color: Color32| {
        let mut mesh = egui::Mesh::default();
        for (share, color) in [
            (edge, Color32::TRANSPARENT),
            (0.5, color),
            (1.0 - edge, Color32::TRANSPARENT),
        ] {
            let x = egui::lerp(rect.left()..=rect.right(), share);
            mesh.colored_vertex(egui::pos2(x, y), color);
            mesh.colored_vertex(egui::pos2(x, y + 1.0), color);
        }
        for segment in 0..2 {
            let index = segment * 2;
            mesh.add_triangle(index, index + 1, index + 2);
            mesh.add_triangle(index + 1, index + 3, index + 2);
        }
        painter.add(egui::Shape::mesh(mesh));
    };
    // The popover shell's glints (`glyph::shell_style`)
    let night_top = Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, 0x57);
    let day_top = Color32::from_rgba_unmultiplied(0x1B, 0x22, 0x3A, 0x10);
    glint(
        rect.top(),
        0.18,
        night_top.lerp_to_gamma(day_top, day_weight),
    );
    let night_bottom = Color32::from_rgba_unmultiplied(0xE1, 0xE6, 0xF5, 0xBF);
    let day_bottom = Color32::from_rgba_unmultiplied(0x6E, 0x8C, 0xD8, 0x8C);
    glint(
        rect.bottom() - 1.0,
        0.12,
        night_bottom.lerp_to_gamma(day_bottom, day_weight),
    );
}

/// With the window open, Twin shows in the Dock and the app switcher like
/// any app; as a menu bar popover it stays out of both
#[cfg(target_os = "macos")]
fn set_dock_presence(shown: bool) {
    use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
    let Some(main_thread) = objc2::MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(main_thread);
    app.setActivationPolicy(if shown {
        NSApplicationActivationPolicy::Regular
    } else {
        NSApplicationActivationPolicy::Accessory
    });
    if shown {
        #[allow(deprecated)]
        app.activateIgnoringOtherApps(true);
    }
}

#[cfg(not(target_os = "macos"))]
fn set_dock_presence(_shown: bool) {}

/// Text cut to `width` with an ellipsis; returns where it went, when cut
fn paint_fitted(
    painter: &egui::Painter,
    pos: egui::Pos2,
    anchor: egui::Align2,
    text: &str,
    font: FontId,
    color: Color32,
    width: f32,
) -> Option<Rect> {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), font, color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(width.max(1.0));
    let galley = painter.layout_job(job);
    let rect = anchor.anchor_size(pos, galley.size());
    let elided = galley.elided;
    painter.galley(rect.min, galley, color);
    elided.then_some(rect)
}

/// The full name over a name cut short
fn name_tooltip(ui: &Ui, cut: Option<Rect>, key: &str, name: &str) {
    if let Some(rect) = cut {
        ui.interact(rect, ui.id().with(("name", key)), Sense::hover())
            .on_hover_text(name);
    }
}

/// Places one widget in `rect`
fn put<R>(ui: &mut Ui, rect: Rect, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::left_to_right(Align::Center)),
        add,
    )
    .inner
}

/// A filled text button for a dialog
fn text_button(ui: &mut Ui, label: &str, fill: Color32, ink: Color32) -> egui::Response {
    let font = kit::font(13.0, theme::SEMIBOLD);
    let width = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font.clone(), ink)
        .size()
        .x
        .max(48.0)
        + 28.0;
    let (rect, response) = ui.allocate_exact_size(vec2(width, 32.0), Sense::click());
    let (hover, _) = kit::overlay(ui, &response);
    ui.painter().rect_filled(rect, 9.0, fill);
    ui.painter().rect_filled(rect, 9.0, hover);
    kit::focus_ring(ui, &response, rect, 9.0);
    ui.painter()
        .text(rect.center(), egui::Align2::CENTER_CENTER, label, font, ink);
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// The Side by side row over a pair: its name, and one button for both
fn pair_header(ui: &mut Ui, label: Color32, look: kit::ButtonLook, tooltip: &str) -> bool {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::hover());
    ui.painter().text(
        egui::pos2(rect.left() + kit::ROW_LEFT, rect.center().y),
        egui::Align2::LEFT_CENTER,
        "Side by side",
        kit::font(12.0, theme::SEMIBOLD),
        label,
    );
    let button = Rect::from_min_size(
        egui::pos2(
            rect.right() - kit::ROW_RIGHT - kit::BUTTON,
            rect.center().y - kit::BUTTON / 2.0,
        ),
        vec2(kit::BUTTON, kit::BUTTON),
    );
    put(ui, button, |ui| {
        kit::square_button(ui, kit::Face::Glyph(pair_glyph(look)), look, tooltip).clicked()
    })
}

fn pair_glyph(look: kit::ButtonLook) -> kit::Glyph {
    if look == kit::ButtonLook::Active {
        kit::Glyph::Stop
    } else {
        kit::Glyph::Play
    }
}

/// Phone rows, 16 points apart, inside the card's padding
fn phones_block(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    ui.add_space(16.0);
    let width = ui.available_width();
    ui.scope(|ui| {
        ui.set_width(width);
        add(ui);
    });
    ui.add_space(16.0);
}

enum Mark<'a> {
    Letter(&'a str),
    Font(&'a str),
    Glyph(kit::Glyph),
}

struct PhoneRow<'a> {
    key: &'a str,
    mark: Mark<'a>,
    name: &'a str,
    subtitle: &'a str,
    connection: kit::Glyph,
    mirroring: bool,
    look: kit::ButtonLook,
    tooltip: &'a str,
}

/// A phone: its mark on a tile, name and version, how it is connected, and
/// its mirror button. Returns whether the button was clicked.
fn phone_row(ui: &mut Ui, row: PhoneRow<'_>) -> bool {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), PHONE_ROW), Sense::hover());
    let painter = ui.painter().clone();
    let tile = Rect::from_min_size(
        egui::pos2(rect.left() + kit::ROW_LEFT, rect.top()),
        vec2(PHONE_ROW, PHONE_ROW),
    );
    kit::paint_tile(&painter, tile, row.mirroring);
    let ink = if row.mirroring {
        theme::green()
    } else {
        theme::tab_inactive()
    };
    paint_mark(&painter, tile, &row.mark, ink);

    let button = Rect::from_min_size(
        egui::pos2(
            rect.right() - kit::ROW_RIGHT - kit::BUTTON,
            rect.center().y - kit::BUTTON / 2.0,
        ),
        vec2(kit::BUTTON, kit::BUTTON),
    );
    let connection = Rect::from_center_size(
        egui::pos2(button.left() - 12.0 - 14.0, rect.center().y),
        vec2(kit::ICON, kit::ICON),
    );
    kit::paint_glyph(&painter, connection, row.connection, theme::icon_muted());
    ui.interact(
        connection,
        ui.id().with(("connection", row.key)),
        Sense::hover(),
    )
    .on_hover_text(if row.connection == kit::Glyph::Wifi {
        "Wi-Fi"
    } else {
        "USB"
    });

    let text_left = tile.right() + 12.0;
    let text_width = connection.left() - 12.0 - text_left;
    let has_subtitle = !row.subtitle.is_empty();
    let cut = paint_fitted(
        &painter,
        egui::pos2(
            text_left,
            if has_subtitle {
                rect.center().y - 9.0
            } else {
                rect.center().y
            },
        ),
        egui::Align2::LEFT_CENTER,
        row.name,
        kit::font(14.0, theme::MEDIUM),
        theme::text_strong(),
        text_width,
    );
    name_tooltip(ui, cut, row.key, row.name);
    if has_subtitle {
        paint_fitted(
            &painter,
            egui::pos2(text_left, rect.center().y + 10.0),
            egui::Align2::LEFT_CENTER,
            row.subtitle,
            kit::regular_font(12.0),
            theme::heading(),
            text_width,
        );
    }
    let glyph = if row.mirroring {
        kit::Glyph::Stop
    } else {
        kit::Glyph::Play
    };
    ui.push_id(("phone", row.key), |ui| {
        put(ui, button, |ui| {
            kit::square_button(ui, kit::Face::Glyph(glyph), row.look, row.tooltip).clicked()
        })
    })
    .inner
}

fn paint_mark(painter: &egui::Painter, rect: Rect, mark: &Mark<'_>, ink: Color32) {
    match mark {
        Mark::Letter(letter) => {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                *letter,
                kit::font(20.0, theme::BOLD),
                ink,
            );
        }
        Mark::Font(glyph) => {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                *glyph,
                theme::icon_font(kit::ICON),
                ink,
            );
        }
        Mark::Glyph(glyph) => {
            kit::paint_glyph(
                painter,
                Rect::from_center_size(rect.center(), vec2(kit::ICON, kit::ICON)),
                *glyph,
                ink,
            );
        }
    }
}

fn mirror_look(mirroring: bool, ready: bool) -> kit::ButtonLook {
    if mirroring {
        kit::ButtonLook::Active
    } else if ready {
        kit::ButtonLook::Normal
    } else {
        kit::ButtonLook::Disabled
    }
}

/// `in_pair`: the button hides or shows this phone's side of an open pair
fn mirror_tooltip(mirroring: bool, in_pair: bool) -> &'static str {
    match (mirroring, in_pair) {
        (true, true) => "Hide from Side by side",
        (false, true) => "Show in Side by side",
        (true, false) => "Stop mirroring",
        (false, false) => "Mirror screen",
    }
}

fn android_phone_row(
    ui: &mut Ui,
    device: &backend::DeviceInfo,
    mirroring: bool,
    mirror_ok: bool,
    in_pair: bool,
) -> bool {
    let subtitle = if device.android_version.is_empty() {
        String::new()
    } else {
        format!("Android {}", device.android_version)
    };
    // Why the button is off: adb's word for the phone's state
    let waiting = format!("The phone is {}", device.state);
    phone_row(
        ui,
        PhoneRow {
            key: &device.mirror_key,
            // Google's G for a Pixel, else the Android head
            mark: if device.name.starts_with("Pixel") {
                Mark::Letter("G")
            } else {
                Mark::Glyph(kit::Glyph::Android)
            },
            name: &device.name,
            subtitle: &subtitle,
            connection: if device.wireless {
                kit::Glyph::Wifi
            } else {
                kit::Glyph::Usb
            },
            mirroring,
            look: mirror_look(mirroring, (device.ready && mirror_ok) || in_pair),
            tooltip: if !device.ready && !mirroring {
                &waiting
            } else if !mirror_ok && !mirroring && !in_pair {
                "Twin Mirror is missing; reinstall Twin to mirror"
            } else {
                mirror_tooltip(mirroring, in_pair)
            },
        },
    )
}

fn iphone_phone_row(
    ui: &mut Ui,
    iphone: &twin_core::dmb::Iphone,
    mirroring: bool,
    in_pair: bool,
) -> bool {
    let subtitle = if iphone.os_version.is_empty() {
        String::new()
    } else {
        // "27.0" reads "iOS 27"
        let version = iphone
            .os_version
            .strip_suffix(".0")
            .unwrap_or(&iphone.os_version);
        format!("iOS {version}")
    };
    phone_row(
        ui,
        PhoneRow {
            key: &iphone.udid,
            mark: Mark::Font(ph::APPLE_LOGO),
            name: &iphone.name,
            subtitle: &subtitle,
            connection: kit::Glyph::Usb,
            mirroring,
            look: mirror_look(mirroring, true),
            tooltip: mirror_tooltip(mirroring, in_pair),
        },
    )
}

fn empty_row(ui: &mut Ui, title: &str, hint: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 64.0), Sense::hover());
    let painter = ui.painter();
    painter.text(
        egui::pos2(rect.left() + kit::ROW_LEFT, rect.center().y - 9.0),
        egui::Align2::LEFT_CENTER,
        title,
        kit::font(13.0, theme::MEDIUM),
        theme::text_bright(),
    );
    paint_fitted(
        painter,
        egui::pos2(rect.left() + kit::ROW_LEFT, rect.center().y + 10.0),
        egui::Align2::LEFT_CENTER,
        hint,
        kit::regular_font(12.0),
        theme::heading(),
        rect.width() - 2.0 * kit::ROW_LEFT,
    );
}

struct VirtualRow<'a> {
    key: &'a str,
    mark: Mark<'a>,
    running: bool,
    name: &'a str,
    buttons: [(kit::Face, kit::ButtonLook, &'a str); 2],
}

/// An emulator or a simulator: its mark, its name and two buttons. Returns
/// the clicked button's index.
fn virtual_row(ui: &mut Ui, row: VirtualRow<'_>) -> Option<usize> {
    let (rect, _) =
        ui.allocate_exact_size(vec2(ui.available_width(), kit::LIST_ROW), Sense::hover());
    let painter = ui.painter().clone();
    let mark = Rect::from_min_size(
        egui::pos2(
            rect.left() + kit::ROW_LEFT,
            rect.center().y - kit::ICON / 2.0,
        ),
        vec2(kit::ICON, kit::ICON),
    );
    paint_mark(
        &painter,
        mark,
        &row.mark,
        if row.running {
            theme::green()
        } else {
            theme::icon_muted()
        },
    );
    let buttons_left = rect.right() - kit::ROW_RIGHT - 2.0 * kit::BUTTON - 8.0;
    let cut = paint_fitted(
        &painter,
        egui::pos2(mark.right() + 12.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        row.name,
        kit::font(14.0, theme::MEDIUM),
        theme::text_strong(),
        buttons_left - 12.0 - (mark.right() + 12.0),
    );
    name_tooltip(ui, cut, row.key, row.name);
    let mut clicked = None;
    ui.push_id(("virtual", row.key), |ui| {
        for (index, (face, look, tooltip)) in row.buttons.into_iter().enumerate() {
            let button = Rect::from_min_size(
                egui::pos2(
                    buttons_left + index as f32 * (kit::BUTTON + 8.0),
                    rect.center().y - kit::BUTTON / 2.0,
                ),
                vec2(kit::BUTTON, kit::BUTTON),
            );
            if put(ui, button, |ui| {
                kit::square_button(ui, face, look, tooltip).clicked()
            }) {
                clicked = Some(index);
            }
        }
    });
    clicked
}

/// A Settings row: its label on the left and `control` on the right
fn setting_row(ui: &mut Ui, label: &str, divider: bool, control: impl FnOnce(&mut Ui)) {
    if divider {
        kit::row_divider(ui);
    }
    let (rect, _) =
        ui.allocate_exact_size(vec2(ui.available_width(), kit::SETTING_ROW), Sense::hover());
    ui.painter().text(
        egui::pos2(rect.left() + kit::ROW_LEFT, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        kit::regular_font(13.0),
        theme::text_bright(),
    );
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(vec2(kit::ROW_LEFT, 0.0)))
            .layout(Layout::right_to_left(Align::Center)),
        control,
    );
}

/// A one-line text field, right-aligned, with an optional unit inside it
fn field(ui: &mut Ui, id: &str, value: &mut String, width: f32, unit: &str) -> bool {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 28.0), Sense::hover());
    let unit_width = if unit.is_empty() { 0.0 } else { 20.0 };
    let edit = Rect::from_min_max(
        egui::pos2(rect.left() + 10.0, rect.top()),
        egui::pos2(rect.right() - 10.0 - unit_width, rect.bottom()),
    );
    let background = ui.painter().add(egui::Shape::Noop);
    let output = ui
        .scope_builder(egui::UiBuilder::new().max_rect(edit), |ui| {
            ui.add_sized(
                edit.size(),
                TextEdit::singleline(value)
                    .id_salt(id)
                    .frame(Frame::NONE)
                    .font(kit::font(13.0, theme::MEDIUM))
                    .text_color(theme::text_strong())
                    .horizontal_align(if unit.is_empty() {
                        Align::Min
                    } else {
                        Align::Max
                    })
                    .vertical_align(Align::Center)
                    .margin(Margin::ZERO),
            )
        })
        .inner;
    let stroke = if output.has_focus() {
        Stroke::new(1.5_f32, theme::input_focus_stroke())
    } else {
        Stroke::new(1.0_f32, theme::field_stroke())
    };
    ui.painter().set(
        background,
        egui::Shape::rect_filled(rect, 7.0, theme::field_fill()),
    );
    ui.painter()
        .rect_stroke(rect, 7.0, stroke, egui::StrokeKind::Inside);
    if !unit.is_empty() {
        ui.painter().text(
            egui::pos2(rect.right() - 10.0, rect.center().y),
            egui::Align2::RIGHT_CENTER,
            unit,
            kit::font(13.0, theme::MEDIUM),
            theme::heading(),
        );
    }
    output.changed()
}

/// A tool Twin uses: Ready, its warning, or a button that opens `get` (an
/// App Store or download page, or the app to open once)
fn tool_row(
    ui: &mut Ui,
    name: &str,
    tool_name: &str,
    divider: bool,
    info: Option<&backend::ToolInfo>,
    get: (&str, &str),
) {
    if divider {
        kit::row_divider(ui);
    }
    let (rect, _) =
        ui.allocate_exact_size(vec2(ui.available_width(), kit::SETTING_ROW), Sense::hover());
    let painter = ui.painter();
    let name_galley = painter.layout_no_wrap(
        name.to_owned(),
        kit::regular_font(13.0),
        theme::text_bright(),
    );
    let name_width = name_galley.size().x;
    painter.galley(
        egui::pos2(
            rect.left() + kit::ROW_LEFT,
            rect.center().y - name_galley.size().y / 2.0,
        ),
        name_galley,
        theme::text_bright(),
    );
    painter.text(
        egui::pos2(
            rect.left() + kit::ROW_LEFT + name_width + 8.0,
            rect.center().y,
        ),
        egui::Align2::LEFT_CENTER,
        format!("· {tool_name}"),
        kit::regular_font(13.0),
        theme::heading(),
    );
    let ready = info.is_some_and(|tool| tool.available && tool.warnings.is_empty());
    let right = egui::pos2(rect.right() - kit::ROW_LEFT, rect.center().y);
    if ready {
        painter.text(
            right,
            egui::Align2::RIGHT_CENTER,
            "Ready",
            kit::font(12.0, theme::MEDIUM),
            theme::green(),
        );
    } else if info.is_some() {
        let (label, target) = get;
        let width = painter
            .layout_no_wrap(
                label.to_owned(),
                kit::font(12.0, theme::MEDIUM),
                Color32::WHITE,
            )
            .size()
            .x
            + 24.0;
        let button = Rect::from_min_size(
            egui::pos2(right.x - width, rect.center().y - 14.0),
            vec2(width, 28.0),
        );
        let response = ui.interact(button, ui.id().with(("get", name)), Sense::click());
        let (hover, _) = kit::overlay(ui, &response);
        ui.painter().rect_filled(button, 8.0, theme::button_fill());
        ui.painter().rect_filled(button, 8.0, hover);
        ui.painter().text(
            button.center(),
            egui::Align2::CENTER_CENTER,
            label,
            kit::font(12.0, theme::MEDIUM),
            theme::button_ink(),
        );
        let warning = info.and_then(|tool| tool.warnings.first()).cloned();
        let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
        let response = match warning {
            Some(warning) => response.on_hover_text(warning),
            None => response,
        };
        if response.clicked() {
            let _ = std::process::Command::new("open").arg(target).spawn();
        }
    }
}

fn screen_in_order(from: Screen, step: isize) -> Option<Screen> {
    let index = SCREEN_ORDER.iter().position(|screen| *screen == from)?;
    SCREEN_ORDER.get(index.checked_add_signed(step)?).copied()
}

fn screen_transition_direction(from: Screen, to: Screen) -> f32 {
    let index = |screen| {
        SCREEN_ORDER
            .iter()
            .position(|candidate| *candidate == screen)
    };
    if index(to) < index(from) { -1.0 } else { 1.0 }
}

fn popover_position(
    anchor: MenuAnchor,
    fallback_monitor_width: Option<f64>,
    displays: &[DisplayBounds],
) -> (f64, f64) {
    let anchor = logical_menu_anchor(anchor, displays);
    let y = anchor.bottom_y + POPOVER_GAP;
    let x = clamp_window_x_to_displays(
        anchor.x - f64::from(theme::WINDOW_WIDTH) / 2.0,
        anchor.x,
        y,
        f64::from(theme::WINDOW_WIDTH),
        WINDOW_MARGIN,
        fallback_monitor_width,
        displays,
    );

    (x, y)
}

fn logical_menu_anchor(anchor: MenuAnchor, displays: &[DisplayBounds]) -> LogicalMenuAnchor {
    let display = display_for_physical_menu_anchor(anchor, displays);
    let scale = display
        .map(|display| display.scale)
        .unwrap_or_else(|| inferred_status_scale(anchor.rect_height));

    let click_x = anchor.click_x / scale;
    let click_y = anchor.click_y / scale;
    let rect_center_x = (anchor.rect_x + anchor.rect_width / 2.0) / scale;
    let rect_bottom_y = (anchor.rect_y + anchor.rect_height) / scale;
    let icon_height = (anchor.rect_height / scale).clamp(16.0, 36.0);

    let rect_matches_click_display = display
        .map(|display| {
            display.contains(rect_center_x, rect_bottom_y)
                && (rect_center_x - click_x).abs() <= 96.0
        })
        .unwrap_or(false);
    let x = if rect_matches_click_display {
        rect_center_x
    } else {
        click_x
    };
    let bottom_y = if rect_matches_click_display {
        rect_bottom_y
    } else {
        click_y + icon_height / 2.0
    };

    LogicalMenuAnchor { x, bottom_y }
}

fn popover_transition_duration(phase: PopoverTransitionPhase) -> Duration {
    match phase {
        PopoverTransitionPhase::Appearing => POPOVER_APPEAR_DURATION,
        PopoverTransitionPhase::Disappearing => POPOVER_HIDE_DURATION,
    }
}

fn popover_transition_opacity(phase: PopoverTransitionPhase, progress: f32) -> f32 {
    let progress = progress.clamp(0.0, 1.0);
    match phase {
        PopoverTransitionPhase::Appearing => egui::emath::easing::cubic_out(progress),
        PopoverTransitionPhase::Disappearing => 1.0 - egui::emath::easing::cubic_in(progress),
    }
}

#[cfg(target_os = "macos")]
fn set_native_window_opacity(frame: &eframe::Frame, opacity: f32) -> bool {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let Ok(handle) = frame.window_handle() else {
        return false;
    };
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return false;
    };
    let ns_view_ptr = handle.ns_view.as_ptr().cast::<objc2_app_kit::NSView>();
    // SAFETY: AppKit supplies this non-null NSView pointer for the lifetime of
    // the eframe window, and it is used only synchronously on the UI thread.
    let Some(ns_view) = (unsafe { ns_view_ptr.as_ref() }) else {
        return false;
    };
    let Some(window) = ns_view.window() else {
        return false;
    };

    window.setAlphaValue(f64::from(opacity.clamp(0.0, 1.0)));
    true
}

#[cfg(not(target_os = "macos"))]
fn set_native_window_opacity(_frame: &eframe::Frame, _opacity: f32) -> bool {
    false
}

/// The window's chrome: a titled, resizable window with the traffic lights
/// over a full-size content view, or the popover's borderless panel
#[cfg(target_os = "macos")]
fn set_native_window_mode(frame: &eframe::Frame, detached: bool) {
    use objc2_app_kit::{NSWindowStyleMask, NSWindowTitleVisibility};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let Ok(handle) = frame.window_handle() else {
        return;
    };
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return;
    };
    let ns_view_ptr = handle.ns_view.as_ptr().cast::<objc2_app_kit::NSView>();
    // SAFETY: as in `set_native_window_opacity`
    let Some(ns_view) = (unsafe { ns_view_ptr.as_ref() }) else {
        return;
    };
    let Some(window) = ns_view.window() else {
        return;
    };
    // The popover's own mask, as winit made it, to return to
    static POPOVER_MASK: Mutex<Option<NSWindowStyleMask>> = Mutex::new(None);
    let mut popover_mask = POPOVER_MASK.lock().unwrap();
    if detached {
        popover_mask.get_or_insert(window.styleMask());
        window.setStyleMask(
            NSWindowStyleMask::Titled
                | NSWindowStyleMask::Closable
                | NSWindowStyleMask::Miniaturizable
                | NSWindowStyleMask::Resizable
                | NSWindowStyleMask::FullSizeContentView,
        );
        window.setTitlebarAppearsTransparent(true);
        window.setTitleVisibility(NSWindowTitleVisibility::Hidden);
    } else {
        window.setStyleMask(popover_mask.unwrap_or(NSWindowStyleMask::Borderless));
        window.setTitlebarAppearsTransparent(false);
    }
    window.setHasShadow(true);
    window.invalidateShadow();
}

#[cfg(not(target_os = "macos"))]
fn set_native_window_mode(_frame: &eframe::Frame, _detached: bool) {}

fn display_for_physical_menu_anchor(
    anchor: MenuAnchor,
    displays: &[DisplayBounds],
) -> Option<DisplayBounds> {
    displays
        .iter()
        .copied()
        .filter(|display| {
            let x = anchor.click_x / display.scale;
            let y = anchor.click_y / display.scale;
            display.contains(x, y)
        })
        .min_by(|left, right| {
            score_display_for_menu_anchor(anchor, *left)
                .total_cmp(&score_display_for_menu_anchor(anchor, *right))
        })
}

fn score_display_for_menu_anchor(anchor: MenuAnchor, display: DisplayBounds) -> f64 {
    let icon_height = anchor.rect_height / display.scale;
    let height_score = if (16.0..=36.0).contains(&icon_height) {
        (icon_height - 22.0).abs()
    } else if icon_height < 16.0 {
        100.0 + (16.0 - icon_height)
    } else {
        100.0 + (icon_height - 36.0)
    };

    let rect_center_x = (anchor.rect_x + anchor.rect_width / 2.0) / display.scale;
    let rect_bottom_y = (anchor.rect_y + anchor.rect_height) / display.scale;
    let rect_score = if display.contains(rect_center_x, rect_bottom_y) {
        0.0
    } else {
        8.0
    };

    height_score + rect_score
}

fn inferred_status_scale(rect_height: f64) -> f64 {
    if rect_height >= 36.0 { 2.0 } else { 1.0 }
}

fn clamp_window_x_to_displays(
    desired_x: f64,
    anchor_x: f64,
    anchor_y: f64,
    window_width: f64,
    margin: f64,
    fallback_monitor_width: Option<f64>,
    displays: &[DisplayBounds],
) -> f64 {
    let display = display_for_anchor(anchor_x, anchor_y, displays)
        .or_else(|| fallback_display_bounds(anchor_x, fallback_monitor_width));
    let Some(display) = display else {
        return desired_x.max(margin);
    };

    let min_x = display.min_x + margin;
    let max_x = display.max_x - window_width - margin;

    if max_x < min_x {
        min_x
    } else {
        desired_x.clamp(min_x, max_x)
    }
}

fn display_for_anchor(
    anchor_x: f64,
    anchor_y: f64,
    displays: &[DisplayBounds],
) -> Option<DisplayBounds> {
    displays
        .iter()
        .copied()
        .find(|display| {
            (display.min_x..=display.max_x).contains(&anchor_x)
                && (display.min_y..=display.max_y).contains(&anchor_y)
        })
        .or_else(|| {
            displays
                .iter()
                .copied()
                .find(|display| (display.min_x..=display.max_x).contains(&anchor_x))
        })
}

fn fallback_display_bounds(
    anchor_x: f64,
    fallback_monitor_width: Option<f64>,
) -> Option<DisplayBounds> {
    let width = fallback_monitor_width.filter(|width| *width > 1.0)?;
    let min_x = (anchor_x / width).floor() * width;

    Some(DisplayBounds {
        min_x,
        max_x: min_x + width,
        min_y: f64::NEG_INFINITY,
        max_y: f64::INFINITY,
        scale: 1.0,
    })
}

#[cfg(target_os = "macos")]
fn active_display_bounds() -> Vec<DisplayBounds> {
    use core_graphics::display::CGDisplay;

    let Ok(display_ids) = CGDisplay::active_displays() else {
        return Vec::new();
    };

    display_ids
        .into_iter()
        .filter_map(|id| {
            let display = CGDisplay::new(id);
            let bounds = display.bounds();
            let min_x = bounds.origin.x;
            let min_y = bounds.origin.y;
            let max_x = min_x + bounds.size.width;
            let max_y = min_y + bounds.size.height;
            let scale = display
                .display_mode()
                .and_then(|mode| {
                    let width = mode.width();
                    (width > 0).then_some(mode.pixel_width() as f64 / width as f64)
                })
                .filter(|scale| scale.is_finite() && *scale > 0.0)
                .unwrap_or(1.0);

            if max_x <= min_x || max_y <= min_y {
                None
            } else {
                Some(DisplayBounds {
                    min_x,
                    max_x,
                    min_y,
                    max_y,
                    scale,
                })
            }
        })
        .collect()
}

#[cfg(not(target_os = "macos"))]
fn active_display_bounds() -> Vec<DisplayBounds> {
    Vec::new()
}

impl eframe::App for App {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    /// ← and → move between pages. Take them before egui sees them, which
    /// would otherwise use them to move focus between controls; text fields
    /// keep them for the caret.
    fn raw_input_hook(&mut self, ctx: &Context, raw_input: &mut egui::RawInput) {
        let editing_text = ctx
            .memory(|memory| memory.focused())
            .is_some_and(|id| egui::text_edit::TextEditState::load(ctx, id).is_some());
        if (!self.visible && !self.detached) || self.pending_avd_delete.is_some() || editing_text {
            return;
        }
        raw_input.events.retain(|event| match event {
            egui::Event::Key {
                key: key @ (egui::Key::ArrowLeft | egui::Key::ArrowRight),
                pressed,
                modifiers,
                ..
            } if modifiers.is_none() => {
                if *pressed {
                    self.pending_page_step =
                        Some(if *key == egui::Key::ArrowLeft { -1 } else { 1 });
                }
                false
            }
            _ => true,
        });
    }

    fn on_exit(&mut self) {
        backend::cancel_pairing(&self.shared);
        backend::stop_all_mirrors(&self.shared);
    }

    fn logic(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        let now = Instant::now();
        self.sync_appearance(ctx, now);
        self.update_skin_transition(ctx, now);

        // Some setups surface the window on launch despite `with_visible(false)`;
        // keep it hidden until the user opens it from the menu bar icon.
        if !self.visible && self.created_at.elapsed() < STARTUP_HIDE {
            ctx.send_viewport_cmd(ViewportCommand::Visible(false));
            ctx.request_repaint_after(Duration::from_millis(50));
        }

        // Apply a queued move before the window is shown (see `show`).
        if self.pending_show {
            self.pending_show = false;
            if let Some(transition) = &mut self.animations.popover
                && transition.phase == PopoverTransitionPhase::Appearing
            {
                transition.started_at = Instant::now();
            }
            ctx.send_viewport_cmd(ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(ViewportCommand::Focus);
        }

        if self.reopen_detached {
            let wait = STARTUP_HIDE.saturating_sub(self.created_at.elapsed());
            if wait.is_zero() {
                self.reopen_detached = false;
                self.detach(ctx);
            } else {
                ctx.request_repaint_after(wait);
            }
        }

        self.handle_events(ctx);
        self.handle_focus(ctx);
        self.handle_escape(ctx);
        self.handle_keyboard_navigation(ctx);
        self.update_popover_transition(ctx);

        // Keep the full UI snapshot current while it is in use. When hidden,
        // a lightweight ADB-only probe is enough to update the menu bar icon.
        if self.last_poll.elapsed() > STATUS_POLL {
            self.last_poll = Instant::now();
            if needs_full_status_refresh(self.visible || self.detached, self.settings.auto_mirror) {
                backend::refresh_status(self.shared.clone(), ctx.clone());
            } else {
                backend::refresh_connection_state(self.shared.clone(), ctx.clone());
            }
        }

        self.maybe_auto_mirror(ctx);
        self.sync_menu_icon();
        // The window's close button: back to the menu bar, not quit
        if self.detached && !self.quitting && ctx.input(|input| input.viewport().close_requested())
        {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.attach(ctx, false);
        }

        let interval = if self.visible || self.detached {
            Duration::from_secs(1)
        } else {
            STATUS_POLL
        };
        ctx.request_repaint_after(interval);

        // A finished pairing holds its "Paired" state for a moment, then
        // returns to the device list as a step back (Main slides in from the
        // left while the Pair page slides out to the right).
        let pairing_done = {
            let state = self.shared.lock().unwrap();
            match state.pairing.as_ref().map(|session| &session.phase) {
                None => true,
                Some(PairingPhase::Paired { at, .. }) => {
                    let remaining = PAIRED_HOLD.saturating_sub(at.elapsed());
                    if !remaining.is_zero() {
                        ctx.request_repaint_after(remaining);
                    }
                    remaining.is_zero()
                }
                Some(_) => false,
            }
        };
        if self.screen == Screen::Pair && pairing_done {
            self.go(Screen::Devices, ctx);
        }
    }

    fn ui(&mut self, ui: &mut Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let rect = ui.max_rect();

        self.apply_window_mode(&ctx, frame);
        if self.pending_attach_show && self.pending_mode.is_none() {
            self.pending_attach_show = false;
            self.show(&ctx, self.menu_anchor());
        }
        if self.detached {
            if self.window_ui(ui, &ctx) {
                self.attach(&ctx, true);
            }
            return;
        }

        let opacity = if !self.visible {
            0.0
        } else if let Some(transition) = self.animations.popover {
            let progress = transition.started_at.elapsed().as_secs_f32()
                / popover_transition_duration(transition.phase).as_secs_f32();
            popover_transition_opacity(transition.phase, progress)
        } else {
            1.0
        };
        if !set_native_window_opacity(frame, opacity) {
            ui.set_opacity(opacity);
        }

        // Beak + rounded body + gradient + hairline, baked into one texture.
        ui.painter().image(
            self.night_shell.id(),
            rect,
            Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        let day_alpha = (self.animations.skin.day_weight * 255.0).round() as u8;
        if day_alpha > 0 {
            ui.painter().image(
                self.day_shell.id(),
                rect,
                Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                Color32::from_white_alpha(day_alpha),
            );
        }

        let content = Rect::from_min_max(
            egui::pos2(
                rect.left() + kit::PAD,
                rect.top() + theme::BEAK_HEIGHT + kit::PAD,
            ),
            egui::pos2(rect.right() - kit::PAD, rect.bottom() - kit::PAD),
        );
        let title = Rect::from_min_size(content.min, vec2(content.width(), TITLE_HEIGHT));
        let tabs = Rect::from_min_size(
            egui::pos2(content.left(), title.bottom() + 16.0),
            vec2(content.width(), TABS_HEIGHT),
        );
        let body = Rect::from_min_max(
            egui::pos2(content.left(), tabs.bottom() + 20.0),
            content.max,
        );

        // The header goes first, so a click on a tab starts its slide in this
        // same frame instead of drawing the new page unmoved for one frame.
        self.popover_header(ui, &ctx, title, tabs);
        let window = Rect::from_min_max(
            egui::pos2(rect.left() + 1.0, body.top() - SCREEN_TOP_BLEED),
            egui::pos2(rect.right() - 1.0, rect.bottom() - 1.0),
        );
        let day_weight = self.animations.skin.day_weight;
        let shell = Arc::clone(&self.shell);
        let fade = move |pos: egui::Pos2| shell.color_at(pos, day_weight);
        self.render_pages(ui, &ctx, body, window, &fade, false);
    }
}

impl App {
    /// The mark, "Twin" and Refresh, then the pages as a segmented control.
    /// Both stay put while the pages slide under them; the control's
    /// selection slides with the pages.
    fn popover_header(&mut self, ui: &mut Ui, ctx: &Context, title: Rect, tabs: Rect) {
        let painter = ui.painter();
        let logo = Rect::from_min_size(
            egui::pos2(title.left() + 4.0, title.center().y - 13.0),
            vec2(26.0, 26.0),
        );
        painter.image(
            self.logo.id(),
            logo,
            Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        painter.text(
            egui::pos2(logo.right() + 12.0, title.center().y),
            egui::Align2::LEFT_CENTER,
            "Twin",
            kit::font(17.0, theme::BOLD),
            theme::text_strong(),
        );
        // In the column of the cards' buttons
        let refresh = Rect::from_min_size(
            egui::pos2(
                title.right() - kit::ROW_RIGHT - kit::BUTTON,
                title.center().y - kit::BUTTON / 2.0,
            ),
            vec2(kit::BUTTON, kit::BUTTON),
        );
        let clicked = ui
            .scope_builder(egui::UiBuilder::new().max_rect(refresh), |ui| {
                kit::ghost_button(ui, kit::Glyph::Refresh, "Refresh").clicked()
            })
            .inner;
        if clicked {
            backend::refresh_status(self.shared.clone(), ctx.clone());
        }

        let labels = SCREEN_ORDER.map(tab_label);
        if let Some(index) = kit::segmented(
            ui,
            tabs,
            &labels,
            screen_index(self.screen),
            self.page_position(),
            12.0,
        ) {
            self.go(SCREEN_ORDER[index], ctx);
        }
    }

    /// The current page, or both pages of a slide side by side. `clip` is
    /// how far content may draw: the surface edge to edge, so hover shapes
    /// and sliding pages run past the content column.
    fn render_pages(
        &mut self,
        ui: &mut Ui,
        ctx: &Context,
        body: Rect,
        clip: Rect,
        fade: &dyn Fn(egui::Pos2) -> Color32,
        in_window: bool,
    ) {
        let transition = self
            .animations
            .screen
            .map(|transition| (transition, transition.progress()));
        if let Some((transition, progress)) = transition
            && progress >= 1.0
        {
            self.animations.screen = None;
            if transition.cancel_pairing_on_complete && self.screen != Screen::Pair {
                backend::cancel_pairing(&self.shared);
            }
        }

        match transition {
            Some((transition, progress)) if progress < 1.0 => {
                let eased = egui::emath::easing::cubic_in_out(progress);
                // A push: both pages stay opaque and move side by side, so
                // they never draw over each other.
                let distance = body.width() + 2.0 * SCROLL_BLEED + 2.0 * kit::PAD;
                let outgoing_x = -transition.direction * distance * eased;
                let incoming_x = transition.direction * distance * (1.0 - eased);
                self.render_screen(
                    ui,
                    body,
                    clip,
                    ctx,
                    transition.from,
                    outgoing_x,
                    fade,
                    in_window,
                );
                self.render_screen(
                    ui,
                    body,
                    clip,
                    ctx,
                    transition.to,
                    incoming_x,
                    fade,
                    in_window,
                );
                ctx.request_repaint();
            }
            _ => self.render_screen(ui, body, clip, ctx, self.screen, 0.0, fade, in_window),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_screen(
        &mut self,
        ui: &mut Ui,
        content: Rect,
        clip: Rect,
        ctx: &Context,
        screen: Screen,
        offset_x: f32,
        fade: &dyn Fn(egui::Pos2) -> Color32,
        in_window: bool,
    ) {
        let mut screen_ui = ui.new_child(
            egui::UiBuilder::new()
                .id_salt(("screen", screen, in_window))
                .max_rect(content.translate(vec2(offset_x, 0.0)))
                .layout(Layout::top_down(Align::Min)),
        );
        screen_ui.set_clip_rect(clip.intersect(ui.clip_rect()));
        if self.animations.screen.is_some() {
            // Both screens are painted during the transition. Keep their
            // visual treatment intact while preventing clicks from reaching
            // either the outgoing or incoming controls mid-animation.
            screen_ui.visuals_mut().disabled_alpha = 1.0;
            screen_ui.disable();
        }

        match screen {
            Screen::Devices => self.devices_page(&mut screen_ui, ctx, fade, in_window),
            Screen::Pair => self.pair_screen(&mut screen_ui, ctx),
            Screen::Logs => self.logs_tab(&mut screen_ui, fade),
            Screen::Settings => self.settings_screen(&mut screen_ui, ctx, fade),
        }
    }

    /// The window: a sidebar with the pages, and the page under its title.
    /// Returns whether "Back to menu bar" was clicked.
    fn window_ui(&mut self, ui: &mut Ui, ctx: &Context) -> bool {
        let rect = ui.max_rect();
        let day_weight = self.animations.skin.day_weight;
        let gradients = self.settings.gradients;
        paint_window_surface(ui.painter(), rect, day_weight, gradients);

        let sidebar = Rect::from_min_size(rect.min, vec2(SIDEBAR_WIDTH, rect.height()));
        let painter = ui.painter();
        painter.rect_filled(sidebar, 0.0, theme::sidebar_fill());
        painter.rect_filled(
            Rect::from_min_max(
                egui::pos2(sidebar.right() - 1.0, sidebar.top()),
                sidebar.max,
            ),
            0.0,
            theme::card_stroke(),
        );
        // The drive server has no title bar: draw where its traffic lights
        // sit, so captures look like the window
        if self.headless {
            for (index, color) in [0xFF5F57, 0xFEBC2E, 0x28C840].into_iter().enumerate() {
                let [_, r, g, b] = (color as u32).to_be_bytes();
                painter.circle_filled(
                    rect.min + vec2(20.0 + 20.0 * index as f32, 22.0),
                    6.0,
                    Color32::from_rgb(r, g, b),
                );
            }
        }
        // Under the traffic lights: the mark and the name
        let brand_y = rect.top() + 64.0;
        let logo = Rect::from_min_size(
            egui::pos2(sidebar.left() + 20.0, brand_y - 12.0),
            vec2(24.0, 24.0),
        );
        painter.image(
            self.logo.id(),
            logo,
            Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        painter.text(
            egui::pos2(logo.right() + 12.0, brand_y),
            egui::Align2::LEFT_CENTER,
            "Twin",
            kit::font(16.0, theme::BOLD),
            theme::text_strong(),
        );

        let nav_left = sidebar.left() + 12.0;
        let nav_width = SIDEBAR_WIDTH - 24.0;
        let nav_top = brand_y + 28.0;
        let position = self.page_position();
        let item = |index: f32| {
            Rect::from_min_size(
                egui::pos2(nav_left, nav_top + index * (NAV_ITEM + 4.0)),
                vec2(nav_width, NAV_ITEM),
            )
        };
        ui.painter()
            .rect_filled(item(position), 8.0, theme::nav_on());
        let mut target = None;
        for (index, screen) in SCREEN_ORDER.iter().enumerate() {
            let rect = item(index as f32);
            let response = ui.interact(rect, ui.id().with(("nav", index)), Sense::click());
            let (hover, _) = kit::overlay(ui, &response);
            if *screen != self.screen {
                ui.painter().rect_filled(rect, 8.0, hover);
            }
            kit::focus_ring(ui, &response, rect, 8.0);
            let under = (1.0 - (position - index as f32).abs()).clamp(0.0, 1.0);
            ui.painter().text(
                egui::pos2(rect.left() + 12.0, rect.center().y),
                egui::Align2::LEFT_CENTER,
                window_title(*screen),
                if under > 0.5 {
                    kit::font(13.0, theme::MEDIUM)
                } else {
                    kit::regular_font(13.0)
                },
                theme::tab_inactive().lerp_to_gamma(theme::text_strong(), under),
            );
            if response
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                target = Some(*screen);
            }
        }
        if let Some(screen) = target {
            self.go(screen, ctx);
        }

        let back_rect = Rect::from_min_size(
            egui::pos2(nav_left, rect.bottom() - 16.0 - kit::BUTTON),
            vec2(nav_width, kit::BUTTON),
        );
        let back = ui
            .scope_builder(egui::UiBuilder::new().max_rect(back_rect), |ui| {
                kit::link_button(ui, kit::Glyph::Back, "Back to menu bar").clicked()
            })
            .inner;

        // The page, under its title and Refresh
        let main = Rect::from_min_max(
            egui::pos2(sidebar.right() + 32.0, rect.top() + 28.0),
            egui::pos2(rect.right() - 32.0, rect.bottom() - 24.0),
        );
        let title = Rect::from_min_size(
            main.min,
            vec2(main.width().min(WINDOW_PAGE_WIDTH), kit::BUTTON),
        );
        let progress = self
            .animations
            .screen
            .map(|transition| (transition, transition.progress()));
        let titles: Vec<(Screen, f32)> = match progress {
            Some((transition, progress)) if progress < 1.0 => vec![
                (transition.from, (1.0 - 2.0 * progress).max(0.0)),
                (transition.to, (2.0 * progress - 1.0).max(0.0)),
            ],
            _ => vec![(self.screen, 1.0)],
        };
        for (screen, opacity) in titles {
            if opacity > 0.0 {
                ui.painter().text(
                    egui::pos2(title.left(), title.center().y),
                    egui::Align2::LEFT_CENTER,
                    window_title(screen),
                    kit::font(20.0, theme::BOLD),
                    theme::text_strong().gamma_multiply(opacity),
                );
            }
        }
        let refresh = Rect::from_min_size(
            egui::pos2(title.right() - kit::ROW_RIGHT - kit::BUTTON, title.top()),
            vec2(kit::BUTTON, kit::BUTTON),
        );
        if ui
            .scope_builder(egui::UiBuilder::new().max_rect(refresh), |ui| {
                kit::ghost_button(ui, kit::Glyph::Refresh, "Refresh").clicked()
            })
            .inner
        {
            backend::refresh_status(self.shared.clone(), ctx.clone());
        }

        // Pages keep a readable width in a wide window, left-aligned under
        // the title
        let body = Rect::from_min_max(
            egui::pos2(main.left(), title.bottom() + 20.0),
            egui::pos2(
                main.right().min(main.left() + WINDOW_PAGE_WIDTH),
                main.bottom(),
            ),
        );
        let clip = Rect::from_min_max(
            egui::pos2(sidebar.right() + 1.0, body.top() - SCREEN_TOP_BLEED),
            rect.max,
        );
        let height = rect.height();
        let fade =
            move |pos: egui::Pos2| window_surface_color(pos.y / height, day_weight, gradients);
        self.render_pages(ui, ctx, body, clip, &fade, true);
        back
    }

    fn devices_page(
        &mut self,
        ui: &mut Ui,
        ctx: &Context,
        fade: &dyn Fn(egui::Pos2) -> Color32,
        in_window: bool,
    ) {
        let body = ui.available_rect_before_wrap();
        let list = if in_window {
            body
        } else {
            Rect::from_min_max(
                body.min,
                egui::pos2(body.right(), body.bottom() - kit::BUTTON - 8.0),
            )
        };
        let mut list_ui = ui.new_child(
            egui::UiBuilder::new()
                .id_salt("devices-list")
                .max_rect(list)
                .layout(Layout::top_down(Align::Min)),
        );
        self.devices_list(&mut list_ui, ctx, fade);

        if !in_window {
            // Its text ends where the cards' buttons do
            let footer = Rect::from_min_max(
                egui::pos2(body.left(), body.bottom() - kit::BUTTON),
                body.max - vec2(kit::ROW_RIGHT - 8.0, 0.0),
            );
            let clicked = ui
                .scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(footer)
                        .layout(Layout::right_to_left(Align::Center)),
                    |ui| kit::link_button(ui, kit::Glyph::Show, "Open as window").clicked(),
                )
                .inner;
            if clicked {
                self.detach(ctx);
            }
        }
    }

    fn devices_list(&mut self, ui: &mut Ui, ctx: &Context, fade: &dyn Fn(egui::Pos2) -> Color32) {
        let (snapshot, (mirrors, pair_keys), starting_avds, deleting_avds, stopping) = {
            let state = self.shared.lock().unwrap();
            let mut mirrors = state.mirrors.keys().cloned().collect::<Vec<_>>();
            mirrors.extend(state.starting_mirrors.iter().cloned());
            let pair_keys = state
                .pair
                .as_ref()
                .map(|pair| pair.keys.to_vec())
                .unwrap_or_default();
            // A phone hidden from the pair is not mirroring
            mirrors.extend(state.pair.iter().flat_map(|pair| pair.shown_keys()));
            (
                state.snapshot.clone(),
                (mirrors, pair_keys),
                state.starting_avds.clone(),
                state.deleting_avds.clone(),
                state.stopping.clone(),
            )
        };

        // The dialog only shows for a listed AVD; one that vanished (deleted
        // elsewhere, or a list not loaded yet) drops its pending delete.
        if let Some(name) = &self.pending_avd_delete
            && !snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot.avds.iter().any(|avd| &avd.name == name))
        {
            self.pending_avd_delete = None;
        }

        let Some(snapshot) = snapshot else {
            ui.add_space(40.0);
            ui.vertical_centered(|ui| {
                ui.add(egui::Spinner::new().size(18.0).color(theme::icon_muted()));
                ui.add_space(12.0);
                ui.add(
                    Label::new(regular("Checking devices…", 12.0, theme::heading()))
                        .selectable(false),
                );
            });
            return;
        };

        let scrcpy_ok = snapshot.mirror.available;
        let settings = &self.settings;
        let shared = &self.shared;
        let pending_avd_delete = &mut self.pending_avd_delete;
        chrome_scroll(ui, "devices-scroll", fade, false, |ui| {
            // Side by side: the first ready Android phone and the first
            // iPhone open together, so they show as one unit with one button
            let pair = settings
                .side_by_side
                .then(|| {
                    let android = snapshot
                        .devices
                        .iter()
                        .find(|device| device.ready && !device.is_emulator)?;
                    Some((android, snapshot.iphones.first()?))
                })
                .flatten();
            let paired_android = pair.map(|(android, _)| android.mirror_key.as_str());
            let androids: Vec<_> = snapshot
                .devices
                .iter()
                .filter(|device| {
                    !device.is_emulator && Some(device.mirror_key.as_str()) != paired_android
                })
                .collect();
            let iphones: Vec<_> = snapshot
                .iphones
                .iter()
                .skip(usize::from(pair.is_some()))
                .collect();

            kit::heading(ui, "Physical", theme::heading());
            kit::card(ui, |ui| {
                if let Some((android, iphone)) = pair {
                    let iphone_key = backend::iphone_mirror_key(iphone);
                    let paired = pair_keys.contains(&android.mirror_key);
                    let android_mirroring = mirrors.contains(&android.mirror_key);
                    let iphone_mirroring = mirrors.contains(&iphone_key);
                    let any = android_mirroring || iphone_mirroring;
                    let both = if any {
                        kit::ButtonLook::Active
                    } else if scrcpy_ok {
                        kit::ButtonLook::Normal
                    } else {
                        kit::ButtonLook::Disabled
                    };
                    let clicked = pair_header(
                        ui,
                        if any {
                            theme::green()
                        } else {
                            theme::heading()
                        },
                        both,
                        if any {
                            "Stop both"
                        } else {
                            "Mirror both side by side"
                        },
                    );
                    if clicked {
                        if paired {
                            backend::stop_pair(shared);
                        } else if any {
                            if android_mirroring {
                                backend::stop_mirror(shared, &android.mirror_key);
                            }
                            if iphone_mirroring {
                                backend::stop_mirror(shared, &iphone_key);
                            }
                        } else {
                            backend::start_pair(
                                shared.clone(),
                                ctx.clone(),
                                android.clone(),
                                iphone.clone(),
                                settings.pair_options(),
                            );
                        }
                    }
                    kit::row_divider(ui);
                    phones_block(ui, |ui| {
                        // In an open pair, a phone's button hides or shows
                        // its side of the window
                        if android_phone_row(ui, android, android_mirroring, scrcpy_ok, paired) {
                            if paired {
                                backend::set_pair_pane(shared, 0, !android_mirroring);
                            } else if android_mirroring {
                                backend::stop_mirror(shared, &android.mirror_key);
                            } else {
                                backend::start_mirror(
                                    shared.clone(),
                                    ctx.clone(),
                                    android.clone(),
                                    settings.scrcpy_options(),
                                    settings.pair_options(),
                                );
                            }
                        }
                        ui.add_space(16.0);
                        if iphone_phone_row(ui, iphone, iphone_mirroring, paired) {
                            if paired {
                                backend::set_pair_pane(shared, 1, !iphone_mirroring);
                            } else if iphone_mirroring {
                                backend::stop_mirror(shared, &iphone_key);
                            } else {
                                backend::start_iphone_mirror(
                                    shared.clone(),
                                    ctx.clone(),
                                    iphone.clone(),
                                    settings.window_height,
                                    &settings.iphone_title,
                                    settings.touch_pointer,
                                );
                            }
                        }
                    });
                    if !androids.is_empty() || !iphones.is_empty() {
                        kit::row_divider(ui);
                    }
                }

                if !androids.is_empty() || !iphones.is_empty() {
                    phones_block(ui, |ui| {
                        let mut first = true;
                        for device in &androids {
                            if !std::mem::take(&mut first) {
                                ui.add_space(16.0);
                            }
                            let mirroring = mirrors.contains(&device.mirror_key);
                            if android_phone_row(ui, device, mirroring, scrcpy_ok, false) {
                                if pair_keys.contains(&device.mirror_key) {
                                    backend::stop_pair(shared);
                                } else if mirroring {
                                    backend::stop_mirror(shared, &device.mirror_key);
                                } else {
                                    backend::start_mirror(
                                        shared.clone(),
                                        ctx.clone(),
                                        (*device).clone(),
                                        settings.scrcpy_options(),
                                        settings.pair_options(),
                                    );
                                }
                            }
                        }
                        for iphone in &iphones {
                            if !std::mem::take(&mut first) {
                                ui.add_space(16.0);
                            }
                            let key = backend::iphone_mirror_key(iphone);
                            let mirroring = mirrors.contains(&key);
                            if iphone_phone_row(ui, iphone, mirroring, false) {
                                if pair_keys.contains(&key) {
                                    backend::stop_pair(shared);
                                } else if mirroring {
                                    backend::stop_mirror(shared, &key);
                                } else {
                                    backend::start_iphone_mirror(
                                        shared.clone(),
                                        ctx.clone(),
                                        (*iphone).clone(),
                                        settings.window_height,
                                        &settings.iphone_title,
                                        settings.touch_pointer,
                                    );
                                }
                            }
                        }
                    });
                } else if pair.is_none() {
                    empty_row(
                        ui,
                        "No phones connected",
                        "Plug one in, or pair one over Wi-Fi in Pair",
                    );
                }
            });

            if snapshot.avds.is_empty() && snapshot.simulators.is_empty() {
                return;
            }
            ui.add_space(16.0);
            kit::heading(ui, "Virtual", theme::heading());
            kit::card(ui, |ui| {
                let mut first = true;
                for avd in &snapshot.avds {
                    if !std::mem::take(&mut first) {
                        kit::row_divider(ui);
                    }
                    let deleting = deleting_avds.contains(&avd.name);
                    let stopping = stopping.contains_key(&avd.name);
                    // Listed by adb in any state (booting, offline): Stop
                    // works through its console either way
                    let running = avd.device.is_some();
                    // Launched and not listed by adb yet
                    let starting = !running && starting_avds.contains(&avd.name);
                    let name = avd.name.replace('_', " ");
                    let delete = if deleting {
                        kit::ButtonLook::Busy
                    } else if running || starting || stopping {
                        kit::ButtonLook::Disabled
                    } else {
                        kit::ButtonLook::Normal
                    };
                    let (main, main_glyph, main_tip) = if starting || stopping {
                        (
                            kit::ButtonLook::Busy,
                            kit::Glyph::Play,
                            if starting {
                                "Starting…"
                            } else {
                                "Stopping…"
                            },
                        )
                    } else if running {
                        (kit::ButtonLook::Active, kit::Glyph::Stop, "Stop emulator")
                    } else if deleting {
                        (kit::ButtonLook::Disabled, kit::Glyph::Play, "")
                    } else {
                        (kit::ButtonLook::Normal, kit::Glyph::Play, "Start emulator")
                    };
                    let row = VirtualRow {
                        key: &avd.name,
                        mark: Mark::Glyph(kit::Glyph::Android),
                        running: running && !stopping,
                        name: &name,
                        buttons: [
                            (
                                kit::Face::Glyph(kit::Glyph::Bin),
                                delete,
                                if deleting {
                                    "Deleting…"
                                } else {
                                    "Delete emulator"
                                },
                            ),
                            (kit::Face::Glyph(main_glyph), main, main_tip),
                        ],
                    };
                    match virtual_row(ui, row) {
                        Some(0) => *pending_avd_delete = Some(avd.name.clone()),
                        Some(1) if running => {
                            if let Some(device) = &avd.device {
                                backend::stop_avd(
                                    shared.clone(),
                                    ctx.clone(),
                                    avd.name.clone(),
                                    device.serial.clone(),
                                );
                            }
                        }
                        Some(1) => {
                            backend::start_avd(shared.clone(), ctx.clone(), avd.name.clone())
                        }
                        _ => {}
                    }
                }
                for sim in &snapshot.simulators {
                    if !std::mem::take(&mut first) {
                        kit::row_divider(ui);
                    }
                    let stopping = stopping.contains_key(&sim.udid);
                    let row = VirtualRow {
                        key: &sim.udid,
                        mark: Mark::Font(ph::APPLE_LOGO),
                        running: !stopping,
                        name: &sim.name,
                        buttons: [
                            (
                                kit::Face::Glyph(kit::Glyph::Show),
                                kit::ButtonLook::Normal,
                                "Show Simulator",
                            ),
                            if stopping {
                                (
                                    kit::Face::Glyph(kit::Glyph::Stop),
                                    kit::ButtonLook::Busy,
                                    "Shutting down…",
                                )
                            } else {
                                (
                                    kit::Face::Glyph(kit::Glyph::Stop),
                                    kit::ButtonLook::Active,
                                    "Shut down simulator",
                                )
                            },
                        ],
                    };
                    match virtual_row(ui, row) {
                        Some(0) => backend::show_simulator(shared, sim),
                        Some(1) => {
                            backend::stop_simulator(shared.clone(), ctx.clone(), sim.clone())
                        }
                        _ => {}
                    }
                }
            });
        });

        if let Some(name) = self.pending_avd_delete.clone() {
            let mut close = false;
            let frame = Frame::new()
                .fill(theme::dialog_fill())
                .stroke(Stroke::new(1.0_f32, theme::card_stroke()))
                .corner_radius(16)
                .inner_margin(Margin::same(20))
                .shadow(egui::Shadow {
                    offset: [0, 12],
                    blur: 32,
                    spread: 0,
                    color: Color32::from_black_alpha(90),
                });
            let modal = egui::Modal::new(egui::Id::new("delete-avd"))
                .frame(frame)
                .backdrop_color(Color32::from_black_alpha(egui::lerp(
                    110.0..=50.0,
                    self.animations.skin.day_weight,
                ) as u8))
                .show(ctx, |ui| {
                    ui.set_width(304.0);
                    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                    ui.add(
                        Label::new(semibold(
                            format!("Delete {}?", name.replace('_', " ")),
                            15.0,
                            theme::text_strong(),
                        ))
                        .selectable(false),
                    );
                    ui.add_space(8.0);
                    ui.add(
                        Label::new(regular(
                            "Its saved data goes with it, for good.",
                            13.0,
                            theme::heading(),
                        ))
                        .wrap()
                        .selectable(false),
                    );
                    ui.add_space(20.0);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing = vec2(8.0, 0.0);
                        if text_button(ui, "Delete", theme::red(), theme::red_ink()).clicked() {
                            backend::delete_avd(self.shared.clone(), ctx.clone(), name.clone());
                            close = true;
                        }
                        if text_button(ui, "Cancel", theme::button_fill(), theme::button_ink())
                            .clicked()
                        {
                            close = true;
                        }
                    });
                });
            if close || modal.should_close() {
                self.pending_avd_delete = None;
            }
        }
    }

    fn logs_tab(&mut self, ui: &mut Ui, fade: &dyn Fn(egui::Pos2) -> Color32) {
        let logs: Vec<String> = self.shared.lock().unwrap().logs.clone();
        let empty = logs.is_empty();
        let mut log_text = if empty {
            "No output yet.".to_string()
        } else {
            logs.join("\n")
        };

        chrome_scroll(ui, "logs-scroll", fade, true, |ui| {
            ui.add(
                TextEdit::multiline(&mut log_text)
                    .desired_width(f32::INFINITY)
                    .desired_rows(logs.len().max(1))
                    .font(FontId::new(10.5, FontFamily::Monospace))
                    .text_color(if empty {
                        theme::heading()
                    } else {
                        theme::text_check()
                    })
                    .frame(Frame::NONE),
            );
        });
    }

    fn settings_screen(
        &mut self,
        ui: &mut Ui,
        ctx: &Context,
        fade: &dyn Fn(egui::Pos2) -> Color32,
    ) {
        // Queried when Settings opens, off the UI thread, so opening it
        // never stutters.
        if self.open_at_login.is_none() && self.login_query.is_none() {
            let (sender, receiver) = std::sync::mpsc::channel();
            let repaint = ctx.clone();
            let headless = self.headless;
            std::thread::spawn(move || {
                let _ = sender.send(!headless && login_item::is_enabled());
                repaint.request_repaint();
            });
            self.login_query = Some(receiver);
        }
        if let Some(enabled) = self
            .login_query
            .as_ref()
            .and_then(|receiver| receiver.try_recv().ok())
        {
            self.open_at_login = Some(enabled);
            self.login_query = None;
        }
        let snapshot = self.shared.lock().unwrap().snapshot.clone();

        let mut changed = false;
        let mut gradients_changed = false;
        let mut theme_changed = false;
        let mut login_changed = None;
        chrome_scroll(ui, "settings-scroll", fade, false, |ui| {
            kit::heading(ui, "Mirror windows", theme::heading());
            kit::card(ui, |ui| {
                setting_row(ui, "Android title", false, |ui| {
                    changed |= field(
                        ui,
                        "android-title",
                        &mut self.settings.window_title,
                        160.0,
                        "",
                    );
                });
                setting_row(ui, "iPhone title", true, |ui| {
                    changed |= field(
                        ui,
                        "iphone-title",
                        &mut self.settings.iphone_title,
                        160.0,
                        "",
                    );
                });
                setting_row(ui, "Height", true, |ui| {
                    changed |= field(ui, "height", &mut self.height_text, 80.0, "pt");
                });
                setting_row(ui, "Gap between phones", true, |ui| {
                    changed |= field(ui, "gap", &mut self.gap_text, 80.0, "pt");
                });
                setting_row(ui, "Side by side", true, |ui| {
                    changed |= kit::toggle(ui, &mut self.settings.side_by_side, "Side by side");
                });
                setting_row(ui, "Touch pointer", true, |ui| {
                    changed |= kit::toggle(ui, &mut self.settings.touch_pointer, "Touch pointer");
                });
                setting_row(ui, "Always on top", true, |ui| {
                    changed |= kit::toggle(ui, &mut self.settings.always_on_top, "Always on top");
                });
                setting_row(ui, "Mirror when connected", true, |ui| {
                    changed |=
                        kit::toggle(ui, &mut self.settings.auto_mirror, "Mirror when connected");
                });
            });

            ui.add_space(16.0);
            kit::heading(ui, "App", theme::heading());
            kit::card(ui, |ui| {
                setting_row(ui, "Appearance", false, |ui| {
                    let modes = [ThemeMode::Auto, ThemeMode::Day, ThemeMode::Night];
                    let selected = modes
                        .iter()
                        .position(|mode| *mode == self.settings.theme)
                        .unwrap_or(0);
                    let (rect, _) = ui.allocate_exact_size(vec2(168.0, 30.0), Sense::hover());
                    let position = ui.ctx().animate_value_with_time(
                        ui.id().with("appearance-pill"),
                        selected as f32,
                        0.18,
                    );
                    if let Some(index) = kit::segmented(
                        ui,
                        rect,
                        &["Auto", "Day", "Night"],
                        selected,
                        position,
                        12.0,
                    ) && index != selected
                    {
                        self.settings.theme = modes[index];
                        theme_changed = true;
                    }
                });
                let mut open_at_login = self.open_at_login.unwrap_or(false);
                setting_row(ui, "Open at login", true, |ui| {
                    if kit::toggle(ui, &mut open_at_login, "Open at login") {
                        login_changed = Some(open_at_login);
                    }
                });
                setting_row(ui, "Gradients", true, |ui| {
                    gradients_changed = kit::toggle(ui, &mut self.settings.gradients, "Gradients");
                });
            });

            ui.add_space(16.0);
            kit::heading(ui, "Setup", theme::heading());
            kit::card(ui, |ui| {
                let xcode = snapshot.as_ref().map(|snapshot| &snapshot.xcode);
                let emulator = snapshot.as_ref().map(|snapshot| &snapshot.emulator);
                // Nothing installs or opens until its button is pressed
                tool_row(
                    ui,
                    "iPhone",
                    "Xcode",
                    false,
                    xcode,
                    if xcode.is_some_and(|tool| tool.available) {
                        ("Open Xcode", "/Applications/Xcode.app")
                    } else {
                        ("Get Xcode", "macappstore://apps.apple.com/app/id497799835")
                    },
                );
                tool_row(
                    ui,
                    "Android emulator",
                    "Android Studio",
                    true,
                    emulator,
                    ("Get Android Studio", "https://developer.android.com/studio"),
                );
            });
            ui.add_space(4.0);
        });

        if let Some(enabled) = login_changed {
            if !self.headless {
                login_item::set_enabled(enabled);
            }
            self.open_at_login = Some(enabled);
            self.login_query = None;
        }
        if gradients_changed {
            changed = true;
            (self.day_shell, self.night_shell, self.shell) =
                load_shells(ctx, self.settings.gradients);
        }
        if changed || theme_changed {
            self.save_settings();
        }
        if theme_changed {
            ctx.request_repaint();
        }
    }

    fn pair_screen(&mut self, ui: &mut Ui, ctx: &Context) {
        let phase = {
            let state = self.shared.lock().unwrap();
            state.pairing.as_ref().map(|session| session.phase.clone())
        };

        let Some(phase) = phase else {
            return;
        };

        match phase {
            PairingPhase::Qr { modules, progress } => {
                let steps = [
                    "Developer options",
                    "Wireless debugging",
                    "Pair device with QR code",
                ];
                let steps_height = steps.len() as f32 * PAIRING_STEP_HEIGHT;
                center_pad(
                    ui,
                    QR_CARD_SIZE + 16.0 + 18.0 + 10.0 + steps_height + 10.0 + 16.0,
                );
                ui.vertical_centered(|ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(vec2(QR_CARD_SIZE, QR_CARD_SIZE), Sense::hover());
                    paint_qr_card(ui, rect, &modules);
                    ui.add_space(16.0);
                    ui.add(
                        Label::new(semibold("Scan with your phone", 13.0, theme::text_bright()))
                            .selectable(false),
                    );
                    ui.add_space(10.0);
                    // The steps stay left-aligned as one block, centered under the code.
                    let block_width = pairing_steps_width(ui, &steps);
                    ui.allocate_ui_with_layout(
                        vec2(block_width, steps_height),
                        Layout::top_down(Align::Min),
                        |ui| {
                            for (index, step) in steps.iter().enumerate() {
                                pairing_step(ui, index + 1, step);
                            }
                        },
                    );
                    ui.add_space(10.0);
                    pairing_progress_block(ui, &progress, 310.0, Align::Center);
                });
            }
            PairingPhase::Connecting { progress } => {
                center_pad(ui, 28.0 + 12.0 + 18.0 + 4.0 + 34.0 + 10.0 + 32.0);
                ui.vertical_centered(|ui| {
                    ui.add(egui::Spinner::new().size(28.0).color(theme::green()));
                    ui.add_space(12.0);
                    ui.add(Label::new(semibold(
                        progress.title.clone(),
                        13.0,
                        theme::text_bright(),
                    )));
                    ui.add_space(4.0);
                    hint_label(ui, &progress.detail, 300.0);
                    ui.add_space(10.0);
                    pairing_progress_block(ui, &progress, 310.0, Align::Center);
                });
            }
            PairingPhase::Paired { device_name, .. } => {
                center_pad(ui, 28.0 + 12.0 + 18.0 + 4.0 + 18.0);
                ui.vertical_centered(|ui| {
                    ui.add(Label::new(icon(ph::CHECK_CIRCLE, 28.0, theme::green())));
                    ui.add_space(12.0);
                    ui.add(Label::new(semibold("Paired", 13.0, theme::text_bright())));
                    ui.add_space(4.0);
                    hint_label(ui, &format!("{device_name} is connected."), 300.0);
                });
            }
            PairingPhase::Failed { message } => {
                center_pad(ui, 28.0 + 12.0 + 18.0 + 4.0 + 32.0 + 36.0);
                ui.vertical_centered(|ui| {
                    ui.add(Label::new(icon(ph::WARNING_CIRCLE, 28.0, theme::red())));
                    ui.add_space(12.0);
                    ui.add(Label::new(semibold(
                        "Pairing failed",
                        13.0,
                        theme::text_bright(),
                    )));
                    ui.add_space(4.0);
                    hint_label(ui, &message, 280.0);
                    ui.add_space(12.0);

                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = vec2(8.0, 0.0);
                        let buttons_width = 2.0 * (48.0 + 28.0) + 8.0;
                        let pad = (ui.available_width() - buttons_width) / 2.0;
                        ui.add_space(pad.max(0.0));
                        if text_button(ui, "Cancel", theme::button_fill(), theme::button_ink())
                            .clicked()
                        {
                            self.go(Screen::Devices, ctx);
                        }
                        if text_button(ui, "Retry", theme::green(), theme::green_ink()).clicked() {
                            backend::start_pairing(self.shared.clone(), ctx.clone());
                        }
                    });
                });
            }
        }
    }
}

#[derive(Clone, Copy, Default)]
struct ScrollChrome {
    offset: f32,
    active_at: f64,
    /// Scrollbar drag not yet applied. It goes through `scroll_with_delta`
    /// on the next frame, like the mouse wheel, because a written offset
    /// would be pulled back to the end by `stick_to_bottom`.
    pending_drag: f32,
}

/// A vertical scroll area with the popover's own chrome: content edges fade
/// into the shell surface, and a thin overlay scrollbar appears while
/// scrolling, widens under the pointer and can be dragged. The viewport
/// bleeds into the side margins so content stays aligned with the header.
fn chrome_scroll(
    ui: &mut Ui,
    salt: &str,
    fade: &dyn Fn(egui::Pos2) -> Color32,
    stick_to_bottom: bool,
    add_contents: impl FnOnce(&mut Ui),
) {
    let column = ui.available_rect_before_wrap();
    let viewport_rect = column.expand2(vec2(SCROLL_BLEED, 0.0));
    let mut area_ui = ui.new_child(
        egui::UiBuilder::new()
            .id_salt(salt)
            .max_rect(viewport_rect)
            .layout(Layout::top_down(Align::Min)),
    );
    // Content stops at the viewport edge, where the fade ends, not the few
    // points past it egui allows by default
    area_ui.visuals_mut().clip_rect_margin = 0.0;
    let chrome_id = area_ui.id().with("chrome");
    let mut chrome: ScrollChrome = ui
        .ctx()
        .data(|data| data.get_temp(chrome_id))
        .unwrap_or_default();
    let pending_drag = std::mem::take(&mut chrome.pending_drag);
    let output = egui::ScrollArea::vertical()
        .id_salt(salt)
        .auto_shrink([false, false])
        .stick_to_bottom(stick_to_bottom)
        .scroll_bar_visibility(ScrollBarVisibility::AlwaysHidden)
        .show(&mut area_ui, |ui| {
            if pending_drag != 0.0 {
                // Unanimated, so the content tracks the pointer exactly.
                ui.scroll_with_delta_animation(
                    vec2(0.0, -pending_drag),
                    egui::style::ScrollAnimation::none(),
                );
            }
            Frame::NONE
                .inner_margin(Margin::symmetric(SCROLL_BLEED as i8, 0))
                .show(ui, |ui| {
                    ui.set_width(column.width());
                    add_contents(ui);
                });
        });
    ui.allocate_rect(column, Sense::hover());

    let ctx = ui.ctx().clone();
    let now = ctx.input(|input| input.time);
    let viewport = output.inner_rect;
    let max_offset = (output.content_size.y - viewport.height()).max(0.0);
    let offset = output.state.offset.y;
    if (offset - chrome.offset).abs() > 0.01 {
        chrome.active_at = now;
    }

    if max_offset > 0.5 {
        let track = Rect::from_min_max(
            egui::pos2(viewport.right() - SCROLLBAR_HIT_WIDTH, viewport.top()),
            viewport.max,
        )
        .shrink2(vec2(0.0, SCROLLBAR_TRACK_INSET));
        let (handle_height, travel) =
            scrollbar_handle(track.height(), viewport.height(), output.content_size.y);

        let bar = ui.interact(track, chrome_id.with("bar"), Sense::DRAG);
        if bar.dragged() {
            let target = (offset + bar.drag_delta().y * max_offset / travel).clamp(0.0, max_offset);
            chrome.pending_drag = target - offset;
            ctx.request_repaint();
        }
        let engaged = bar.hovered() || bar.dragged();
        if engaged {
            chrome.active_at = now;
        }
        let since = now - chrome.active_at;
        let visibility = if since <= SCROLLBAR_LINGER {
            1.0
        } else {
            (1.0 - (since - SCROLLBAR_LINGER) / SCROLLBAR_FADE).clamp(0.0, 1.0) as f32
        };
        if visibility > 0.0 {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        let widen = ctx.animate_bool_with_time(chrome_id.with("widen"), engaged, 0.12);

        let painter = ui.painter().with_clip_rect(viewport);
        let top_strength = (offset / SCROLL_EDGE_FADE_HEIGHT).clamp(0.0, 1.0);
        let bottom_strength = ((max_offset - offset) / SCROLL_EDGE_FADE_HEIGHT).clamp(0.0, 1.0);
        let fade = |top: f32, strength: f32, solid_at_top: bool| {
            let rect = Rect::from_min_max(
                egui::pos2(viewport.left(), top),
                egui::pos2(viewport.right(), top + SCROLL_EDGE_FADE_HEIGHT),
            );
            paint_edge_fade(&painter, fade, rect, solid_at_top, strength);
        };
        if top_strength > 0.0 {
            fade(viewport.top(), top_strength, true);
        }
        if bottom_strength > 0.0 {
            fade(
                viewport.bottom() - SCROLL_EDGE_FADE_HEIGHT,
                bottom_strength,
                false,
            );
        }

        if visibility > 0.0 {
            let width = egui::lerp(SCROLLBAR_WIDTH..=SCROLLBAR_HOVER_WIDTH, widen);
            let top = track.top() + travel * (offset / max_offset).clamp(0.0, 1.0);
            let handle = Rect::from_min_size(
                egui::pos2(viewport.right() - 2.0 - width, top),
                vec2(width, handle_height),
            );
            let color =
                theme::scrollbar_thumb().lerp_to_gamma(theme::scrollbar_thumb_hover(), widen);
            painter.rect_filled(handle, width / 2.0, with_opacity(color, visibility));
        }
    }

    chrome.offset = offset;
    ctx.data_mut(|data| data.insert_temp(chrome_id, chrome));
}

/// Handle length, proportional to the visible share of the content but never
/// shorter than a comfortable grab target, and the distance it can travel.
fn scrollbar_handle(track_height: f32, viewport_height: f32, content_height: f32) -> (f32, f32) {
    let handle = (track_height * viewport_height / content_height)
        .clamp(SCROLLBAR_MIN_HANDLE.min(track_height), track_height);
    (handle, (track_height - handle).max(1.0))
}

/// Fades content into the shell along one edge of `rect`, sampling the shell
/// across the width so the band matches the lit surface beneath it.
fn paint_edge_fade(
    painter: &egui::Painter,
    fade: &dyn Fn(egui::Pos2) -> Color32,
    rect: Rect,
    solid_at_top: bool,
    strength: f32,
) {
    let columns = ((rect.width() / 12.0).ceil() as u32).max(1);
    let edge_y = if solid_at_top {
        rect.top()
    } else {
        rect.bottom()
    };
    let mut mesh = egui::Mesh::default();
    for column in 0..=columns {
        let x = rect.left() + rect.width() * column as f32 / columns as f32;
        let solid = fade(egui::pos2(x, edge_y)).gamma_multiply(strength);
        let (top, bottom) = if solid_at_top {
            (solid, Color32::TRANSPARENT)
        } else {
            (Color32::TRANSPARENT, solid)
        };
        mesh.colored_vertex(egui::pos2(x, rect.top()), top);
        mesh.colored_vertex(egui::pos2(x, rect.bottom()), bottom);
    }
    for column in 0..columns {
        let index = column * 2;
        mesh.add_triangle(index, index + 1, index + 2);
        mesh.add_triangle(index + 1, index + 3, index + 2);
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// Whether focus last moved by keyboard, so focus highlights show for Tab
/// navigation but not after a click.
fn keyboard_focus_id() -> egui::Id {
    egui::Id::new("twin-keyboard-focus")
}

fn paint_qr_card(ui: &Ui, rect: Rect, modules: &twin_core::qr::QrModules) {
    let painter = ui.painter();
    painter.rect_filled(rect, 14.0, theme::qr_card());
    painter.rect_stroke(
        rect,
        14.0,
        Stroke::new(1.0_f32, theme::qr_card_stroke()),
        egui::StrokeKind::Inside,
    );

    // Reserve a 4-module quiet zone; the raw matrix has none and Android's
    // scanner rejects codes without it.
    let qr_size = rect.width() - 20.0;
    let quiet = 4.0;
    let cell = qr_size / (modules.size as f32 + 2.0 * quiet);
    let origin =
        rect.center() - vec2(qr_size / 2.0, qr_size / 2.0) + vec2(quiet * cell, quiet * cell);
    for y in 0..modules.size {
        for x in 0..modules.size {
            if modules.dark[y * modules.size + x] {
                let min = origin + vec2(x as f32 * cell, y as f32 * cell);
                painter.rect_filled(
                    Rect::from_min_size(min, vec2(cell + 0.3, cell + 0.3)),
                    0.0,
                    theme::qr_ink(),
                );
            }
        }
    }
}

const PAIRING_STEP_HEIGHT: f32 = 22.0;
const PAIRING_BADGE_SIZE: f32 = 17.0;
const PAIRING_BADGE_GAP: f32 = 8.0;

fn pairing_step_font() -> FontId {
    FontId::new(11.5, FontFamily::Proportional)
}

/// Width of the widest numbered step, so the steps center as one block.
fn pairing_steps_width(ui: &Ui, steps: &[&str]) -> f32 {
    let text = steps
        .iter()
        .map(|step| {
            ui.painter()
                .layout_no_wrap((*step).to_owned(), pairing_step_font(), theme::text_check())
                .size()
                .x
        })
        .fold(0.0, f32::max);
    PAIRING_BADGE_SIZE + PAIRING_BADGE_GAP + text.ceil()
}

/// A numbered step of the phone-side pairing path.
fn pairing_step(ui: &mut Ui, number: usize, text: &str) {
    ui.horizontal(|ui| {
        ui.set_height(PAIRING_STEP_HEIGHT);
        let (badge, _) =
            ui.allocate_exact_size(vec2(PAIRING_BADGE_SIZE, PAIRING_BADGE_SIZE), Sense::hover());
        ui.painter()
            .circle_filled(badge.center(), 8.5, theme::control_selected());
        ui.painter().text(
            badge.center(),
            egui::Align2::CENTER_CENTER,
            number.to_string(),
            FontId::new(10.0, FontFamily::Name(theme::SEMIBOLD.into())),
            theme::text_bright(),
        );
        ui.add_space(PAIRING_BADGE_GAP);
        ui.add(
            Label::new(
                egui::RichText::new(text)
                    .font(pairing_step_font())
                    .color(theme::text_check()),
            )
            .selectable(false),
        );
    });
}

fn with_opacity(color: Color32, opacity: f32) -> Color32 {
    let [red, green, blue, alpha] = color.to_srgba_unmultiplied();
    Color32::from_rgba_unmultiplied(
        red,
        green,
        blue,
        (f32::from(alpha) * opacity.clamp(0.0, 1.0)).round() as u8,
    )
}

fn center_pad(ui: &mut Ui, content_height: f32) {
    let pad = (ui.available_height() - content_height) / 2.0;
    ui.add_space(pad.max(0.0));
}

fn hint_label(ui: &mut Ui, text: &str, width: f32) {
    ui.scope(|ui| {
        ui.set_max_width(width);
        ui.add(
            Label::new(regular(text, 11.0, theme::text_soft()))
                .halign(egui::Align::Center)
                .wrap(),
        );
    });
}

/// Countdown with a timer icon (and the attempt, when retrying), then the
/// endpoint being paired underneath.
fn pairing_progress_block(ui: &mut Ui, progress: &PairingProgress, width: f32, align: Align) {
    if progress.deadline.is_some() {
        ui.ctx().request_repaint_after(Duration::from_secs(1));
    }

    let mut meta = Vec::new();
    if let Some(deadline) = progress.deadline {
        meta.push(format!("{}s", display_remaining_seconds(deadline)));
    }
    if let Some(attempt) = progress.attempt {
        meta.push(format!("attempt {attempt}"));
    }
    if !meta.is_empty() {
        let text = meta.join(" · ");
        let font = FontId::new(11.5, FontFamily::Name(theme::MEDIUM.into()));
        let text_width = ui
            .painter()
            .layout_no_wrap(text.clone(), font.clone(), theme::green())
            .size()
            .x;
        let icon_width = 13.0 + 5.0;
        let size = vec2(icon_width + text_width, 16.0);
        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
        let rect = if align == Align::Center {
            Rect::from_center_size(rect.center(), size)
        } else {
            rect
        };
        let painter = ui.painter();
        painter.text(
            egui::pos2(rect.left(), rect.center().y),
            egui::Align2::LEFT_CENTER,
            if progress.deadline.is_some() {
                ph::TIMER
            } else {
                ph::ARROWS_CLOCKWISE
            },
            theme::icon_font(13.0),
            theme::green(),
        );
        painter.text(
            egui::pos2(rect.left() + icon_width, rect.center().y),
            egui::Align2::LEFT_CENTER,
            text,
            font,
            theme::green(),
        );
    }

    if let Some(endpoint) = &progress.endpoint {
        ui.add_space(4.0);
        ui.scope(|ui| {
            ui.set_max_width(width);
            ui.add(
                Label::new(regular(endpoint, 11.0, theme::heading()))
                    .halign(align)
                    .wrap(),
            );
        });
    }
}

fn display_remaining_seconds(deadline: Instant) -> u64 {
    let remaining = deadline.saturating_duration_since(Instant::now());

    if remaining.is_zero() {
        0
    } else {
        remaining.as_secs() + u64::from(remaining.subsec_nanos() > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIDTH: f64 = 420.0;
    const MARGIN: f64 = 8.0;

    #[test]
    fn interaction_overlay_fades_fully_to_transparent() {
        let hidden = kit::overlay_color(0.0, 0.0);
        let hovered = kit::overlay_color(1.0, 0.0);
        let pressed = kit::overlay_color(1.0, 1.0);

        assert_eq!(hidden, Color32::TRANSPARENT);
        assert!(hovered.a() > hidden.a());
        assert!(pressed.a() > hovered.a());
    }

    #[test]
    fn automatic_theme_follows_the_system() {
        assert_eq!(
            resolved_appearance(ThemeMode::Auto, Some(Theme::Light)),
            theme::Appearance::Day
        );
        assert_eq!(
            resolved_appearance(ThemeMode::Auto, Some(Theme::Dark)),
            theme::Appearance::Night
        );
    }

    #[test]
    fn explicit_theme_overrides_the_system() {
        assert_eq!(
            resolved_appearance(ThemeMode::Day, Some(Theme::Dark)),
            theme::Appearance::Day
        );
        assert_eq!(
            resolved_appearance(ThemeMode::Night, Some(Theme::Light)),
            theme::Appearance::Night
        );
    }

    #[test]
    fn automatic_theme_falls_back_to_night_when_unknown() {
        assert_eq!(
            resolved_appearance(ThemeMode::Auto, None),
            theme::Appearance::Night
        );
    }

    #[test]
    fn committing_skin_preserves_overlapping_screen_and_popover_animations() {
        let started_at = Instant::now();
        let screen = ScreenTransition {
            from: Screen::Devices,
            to: Screen::Settings,
            direction: 1.0,
            started_at,
            cancel_pairing_on_complete: false,
            from_position: 0.0,
        };
        let popover = PopoverTransition {
            phase: PopoverTransitionPhase::Appearing,
            started_at,
        };
        let mut animations = AnimationState {
            screen: Some(screen),
            popover: Some(popover),
            skin: SkinState {
                appearance: theme::Appearance::Night,
                day_weight: 0.75,
                transition: Some(SkinTransition {
                    from_day_weight: 0.0,
                    target: theme::Appearance::Day,
                    started_at,
                }),
            },
        };

        assert!(animations.commit_skin(theme::Appearance::Day));
        assert_eq!(animations.skin.appearance, theme::Appearance::Day);
        assert_eq!(animations.skin.day_weight, 1.0);
        assert!(animations.skin.transition.is_none());
        assert_eq!(animations.screen, Some(screen));
        assert_eq!(animations.popover, Some(popover));

        assert!(!animations.commit_skin(theme::Appearance::Day));
        assert_eq!(animations.screen, Some(screen));
        assert_eq!(animations.popover, Some(popover));
    }

    #[test]
    fn skin_transition_commits_once_at_the_endpoint() {
        let started_at = Instant::now();
        let mut animations = AnimationState::new(theme::Appearance::Night);
        assert!(
            animations
                .skin
                .set_target_at(theme::Appearance::Day, started_at)
        );

        let before = animations
            .advance_skin_at(started_at + SKIN_TRANSITION_DURATION - Duration::from_nanos(1));
        assert!(before.animating);
        assert_eq!(before.committed, None);

        let endpoint = animations.advance_skin_at(started_at + SKIN_TRANSITION_DURATION);
        assert!(!endpoint.animating);
        assert_eq!(endpoint.committed, Some(theme::Appearance::Day));
        assert_eq!(endpoint.day_weight, 1.0);

        let after = animations
            .advance_skin_at(started_at + SKIN_TRANSITION_DURATION + Duration::from_millis(1));
        assert!(!after.animating);
        assert_eq!(after.committed, None);
        assert_eq!(after.day_weight, 1.0);
    }

    #[test]
    fn screen_transitions_push_forward_and_pull_back() {
        assert_eq!(
            screen_transition_direction(Screen::Devices, Screen::Settings),
            1.0
        );
        assert_eq!(screen_transition_direction(Screen::Pair, Screen::Logs), 1.0);
        assert_eq!(
            screen_transition_direction(Screen::Settings, Screen::Devices),
            -1.0
        );
    }

    #[test]
    fn popover_fades_in_and_out_without_overshooting_opacity() {
        assert_eq!(
            popover_transition_opacity(PopoverTransitionPhase::Appearing, 0.0),
            0.0
        );
        assert_eq!(
            popover_transition_opacity(PopoverTransitionPhase::Appearing, 1.0),
            1.0
        );
        assert_eq!(
            popover_transition_opacity(PopoverTransitionPhase::Disappearing, 0.0),
            1.0
        );
        assert_eq!(
            popover_transition_opacity(PopoverTransitionPhase::Disappearing, 1.0),
            0.0
        );
        assert!(popover_transition_opacity(PopoverTransitionPhase::Appearing, 0.5) > 0.5);
        assert!(popover_transition_opacity(PopoverTransitionPhase::Disappearing, 0.5) > 0.5);
    }

    #[test]
    fn scrollbar_handle_tracks_the_visible_share_with_a_minimum_size() {
        assert_eq!(scrollbar_handle(200.0, 200.0, 400.0), (100.0, 100.0));
        assert_eq!(
            scrollbar_handle(200.0, 200.0, 10_000.0).0,
            SCROLLBAR_MIN_HANDLE
        );
    }

    #[test]
    fn arrow_keys_walk_pages_in_order_without_wrapping() {
        assert_eq!(screen_in_order(Screen::Devices, 1), Some(Screen::Pair));
        assert_eq!(screen_in_order(Screen::Logs, 1), Some(Screen::Settings));
        assert_eq!(screen_in_order(Screen::Settings, 1), None);
        assert_eq!(screen_in_order(Screen::Pair, -1), Some(Screen::Devices));
        assert_eq!(screen_in_order(Screen::Devices, -1), None);
    }

    #[test]
    fn ready_mirror_keys_ignore_offline_devices() {
        let devices = [
            device_info("adb-ready._adb-tls-connect._tcp", true),
            device_info("adb-offline._adb-tls-connect._tcp", false),
        ];

        let keys = ready_device_mirror_keys(&devices);

        assert!(keys.contains("adb-ready._adb-tls-connect._tcp"));
        assert!(!keys.contains("adb-offline._adb-tls-connect._tcp"));
    }

    #[test]
    fn hidden_launch_uses_lightweight_status_refresh_without_auto_mirror() {
        assert!(!needs_full_status_refresh(false, false));
        assert!(needs_full_status_refresh(true, false));
        assert!(needs_full_status_refresh(false, true));
    }

    #[test]
    fn clamps_primary_display_edges() {
        let displays = [DisplayBounds {
            min_x: 0.0,
            max_x: 1512.0,
            min_y: 0.0,
            max_y: 982.0,
            scale: 2.0,
        }];

        let x = clamp_window_x_to_displays(1290.0, 1480.0, 24.0, WIDTH, MARGIN, None, &displays);

        assert_eq!(x, 1084.0);
    }

    #[test]
    fn keeps_popover_on_secondary_display() {
        let displays = [
            DisplayBounds {
                min_x: 0.0,
                max_x: 1512.0,
                min_y: 0.0,
                max_y: 982.0,
                scale: 2.0,
            },
            DisplayBounds {
                min_x: 1512.0,
                max_x: 3024.0,
                min_y: 0.0,
                max_y: 982.0,
                scale: 2.0,
            },
        ];

        let x = clamp_window_x_to_displays(2010.0, 2200.0, 24.0, WIDTH, MARGIN, None, &displays);

        assert_eq!(x, 2010.0);
    }

    #[test]
    fn anchors_to_status_item_center_regardless_of_click_position() {
        let displays = [DisplayBounds {
            min_x: 0.0,
            max_x: 1512.0,
            min_y: 0.0,
            max_y: 982.0,
            scale: 2.0,
        }];
        let anchor = |click_x| MenuAnchor {
            rect_x: 900.0 * 2.0,
            rect_y: 0.0,
            rect_width: 32.0 * 2.0,
            rect_height: 22.0 * 2.0,
            click_x: click_x * 2.0,
            click_y: 12.0 * 2.0,
        };

        let left_click = popover_position(anchor(904.0), None, &displays);
        let center_click = popover_position(anchor(916.0), None, &displays);
        let right_click = popover_position(anchor(928.0), None, &displays);

        assert_eq!(left_click, center_click);
        assert_eq!(right_click, center_click);
        assert_eq!(center_click, (706.0, 28.5));
    }

    #[test]
    fn anchors_to_clicked_display_when_status_rect_is_stale() {
        let displays = [
            DisplayBounds {
                min_x: 0.0,
                max_x: 1728.0,
                min_y: 0.0,
                max_y: 1117.0,
                scale: 2.0,
            },
            DisplayBounds {
                min_x: -567.0,
                max_x: 2313.0,
                min_y: -1620.0,
                max_y: 0.0,
                scale: 2.0,
            },
        ];
        let anchor = MenuAnchor {
            rect_x: 1640.0 * 2.0,
            rect_y: 0.0,
            rect_width: 32.0 * 2.0,
            rect_height: 22.0 * 2.0,
            click_x: -420.0 * 2.0,
            click_y: -1610.0 * 2.0,
        };

        let (x, y) = popover_position(anchor, None, &displays);

        assert_eq!(x, -559.0);
        assert_eq!(y, -1592.5);
    }

    #[test]
    fn prefers_status_icon_scale_for_overlapping_physical_coordinates() {
        let displays = [
            DisplayBounds {
                min_x: 0.0,
                max_x: 1512.0,
                min_y: 0.0,
                max_y: 982.0,
                scale: 2.0,
            },
            DisplayBounds {
                min_x: 1512.0,
                max_x: 3024.0,
                min_y: 0.0,
                max_y: 982.0,
                scale: 1.0,
            },
        ];
        let anchor = MenuAnchor {
            rect_x: 2184.0,
            rect_y: 0.0,
            rect_width: 32.0,
            rect_height: 22.0,
            click_x: 2200.0,
            click_y: 12.0,
        };

        let logical = logical_menu_anchor(anchor, &displays);

        assert_eq!(logical.x, 2200.0);
        assert_eq!(logical.bottom_y, 22.0);
    }

    #[test]
    fn falls_back_to_anchor_coordinate_span_without_native_displays() {
        let x = clamp_window_x_to_displays(2010.0, 2200.0, 24.0, WIDTH, MARGIN, Some(1512.0), &[]);

        assert_eq!(x, 2010.0);
    }

    fn device_info(mirror_key: &str, ready: bool) -> backend::DeviceInfo {
        backend::DeviceInfo {
            serial: mirror_key.to_string(),
            mirror_key: mirror_key.to_string(),
            name: "Pixel".to_string(),
            ready,
            state: if ready { "device" } else { "offline" }.to_string(),
            is_emulator: false,
            android_version: String::new(),
            wireless: false,
        }
    }
}
