#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod backend;
mod config;
#[cfg(feature = "drive")]
mod drive;
mod glyph;
mod login_item;
mod mock;
#[cfg(target_os = "macos")]
mod status_click;
mod theme;

use eframe::egui;

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    // `--login-item [on|off]` sets Open at Login, then prints it, for checks
    if let Some(index) = args.iter().position(|arg| arg == "--login-item") {
        match args.get(index + 1).map(String::as_str) {
            Some("on") => login_item::set_enabled(true),
            Some("off") => login_item::set_enabled(false),
            _ => {}
        }
        println!("open at login: {}", login_item::is_enabled());
        return Ok(());
    }
    #[cfg(feature = "drive")]
    {
        if let [_, flag, socket] = args.as_slice()
            && flag == "--drive"
        {
            return drive::serve(std::path::Path::new(socket)).map_err(drive_error);
        }
        if let [_, command, socket, rest @ ..] = args.as_slice()
            && command == "drive"
        {
            return drive::send(std::path::Path::new(socket), &rest.join(" ")).map_err(drive_error);
        }
    }

    if let Some(index) = args.iter().position(|arg| arg == "--render-shell") {
        let path = args
            .get(index + 1)
            .map(String::as_str)
            .unwrap_or("shell.png");
        let appearance = match args.get(index + 2).map(String::as_str) {
            Some("day") => theme::Appearance::Day,
            _ => theme::Appearance::Night,
        };
        std::fs::write(
            path,
            glyph::shell_background_png(3, appearance, theme::WINDOW_FULL_HEIGHT),
        )
        .expect("write shell");
        println!("wrote {path}");
        return Ok(());
    }

    if let Some(index) = args.iter().position(|arg| arg == "--render-menubar") {
        let path = args
            .get(index + 1)
            .map(String::as_str)
            .unwrap_or("menubar.png");
        std::fs::write(path, glyph::menubar_preview_png()).expect("write menubar preview");
        println!("wrote {path}");
        return Ok(());
    }

    let viewport = egui::ViewportBuilder::default()
        .with_title("Twin")
        .with_inner_size([theme::WINDOW_WIDTH, theme::WINDOW_FULL_HEIGHT])
        .with_decorations(false)
        .with_resizable(false)
        .with_transparent(true)
        .with_window_level(egui::WindowLevel::AlwaysOnTop)
        .with_visible(false);

    #[allow(unused_mut)]
    let mut native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};

        native_options.event_loop_builder = Some(Box::new(|builder| {
            builder.with_activation_policy(ActivationPolicy::Accessory);
        }));
    }

    eframe::run_native(
        "Twin",
        native_options,
        Box::new(|cc| {
            app::App::new(cc)
                .map(|app| Box::new(app) as Box<dyn eframe::App>)
                .map_err(|error| -> Box<dyn std::error::Error + Send + Sync> {
                    format!("failed to start twin-app: {error:#}").into()
                })
        }),
    )
}

#[cfg(feature = "drive")]
fn drive_error(error: anyhow::Error) -> eframe::Error {
    eprintln!("twin drive: {error:#}");
    std::process::exit(1)
}
