use std::fs;
use std::path::PathBuf;

use awb_core::scrcpy::ScrcpyOptions;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    #[default]
    Auto,
    Day,
    Night,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub window_title: String,
    /// The iPhone mirror's window title.
    pub iphone_title: String,
    pub window_width: u32,
    pub window_height: u32,
    pub always_on_top: bool,
    /// Launch scrcpy as a borderless (plain) window with no title bar.
    pub borderless: bool,
    /// Auto-start mirroring when a physical phone connects (never emulators).
    pub auto_mirror: bool,
    /// Mirroring a physical phone also opens the other platform's, side by
    /// side `gap` points apart, and the two move together.
    pub side_by_side: bool,
    pub gap: u32,
    /// A round touch pointer over the phones instead of macOS's.
    pub touch_pointer: bool,
    /// Follow the system appearance or force the light/dark app palette.
    pub theme: ThemeMode,
    /// Light the popover surface with gradients and glows; off draws it flat.
    pub gradients: bool,
}

impl Default for Settings {
    fn default() -> Self {
        let defaults = ScrcpyOptions::default();

        Self {
            window_title: defaults.window_title,
            iphone_title: "iPhone".to_string(),
            window_width: defaults.window_width,
            window_height: defaults.window_height,
            always_on_top: true,
            borderless: true,
            auto_mirror: false,
            side_by_side: false,
            gap: 12,
            touch_pointer: true,
            theme: ThemeMode::Auto,
            gradients: true,
        }
    }
}

impl Settings {
    pub fn scrcpy_options(&self) -> ScrcpyOptions {
        ScrcpyOptions {
            borderless: self.borderless,
            always_on_top: self.always_on_top,
            window_title: self.window_title.clone(),
            window_width: self.window_width,
            window_height: self.window_height,
            side_by_side_gap: self.side_by_side.then_some(self.gap),
            touch_pointer: self.touch_pointer,
            ..ScrcpyOptions::default()
        }
    }

    pub fn load() -> Self {
        let mut settings: Self = config_path()
            .and_then(|path| fs::read_to_string(path).ok())
            .and_then(|raw| toml::from_str(&raw).ok())
            .unwrap_or_default();
        // 2.1.0 saved its default title, a phone's model, as the Android title
        if settings.window_title == "Pixel 10 Pro" {
            settings.window_title = ScrcpyOptions::default().window_title;
        }
        settings
    }

    pub fn save(&self) {
        let Some(path) = config_path() else {
            return;
        };

        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }

        if let Ok(raw) = toml::to_string_pretty(self) {
            let _ = fs::write(path, raw);
        }
    }
}

fn config_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;

    Some(base.join("awb").join("config.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_automatic_theme() {
        assert_eq!(Settings::default().theme, ThemeMode::Auto);
    }

    #[test]
    fn old_configs_without_theme_migrate_to_auto() {
        let settings: Settings = toml::from_str(
            r#"
window_title = "Phone"
window_width = 480
window_height = 1071
always_on_top = true
borderless = true
auto_mirror = false
"#,
        )
        .expect("legacy settings should deserialize");

        assert_eq!(settings.theme, ThemeMode::Auto);
        assert!(settings.gradients);
    }

    #[test]
    fn theme_modes_use_stable_lowercase_values() {
        let settings = Settings {
            theme: ThemeMode::Day,
            ..Settings::default()
        };

        let raw = toml::to_string(&settings).expect("settings serialize");
        assert!(raw.contains("theme = \"day\""));
    }
}
