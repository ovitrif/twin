//! iPhones on USB through [imb](https://github.com/ovitrif/imb): listing them and
//! opening their mirror, which imb draws in the same window style as scrcpy's.

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::command_path::resolve_program;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Iphone {
    pub udid: String,
    pub name: String,
    pub model: String,
}

#[derive(Deserialize)]
struct Status {
    devices: Vec<Iphone>,
}

pub struct Imb {
    path: PathBuf,
}

impl Imb {
    pub fn resolve() -> Result<Self> {
        Ok(Self {
            path: resolve_program("imb", None)?,
        })
    }

    pub fn iphones(&self) -> Result<Vec<Iphone>> {
        let output = Command::new(&self.path)
            .args(["status", "--json"])
            .stdin(Stdio::null())
            .output()
            .context("running imb status")?;
        if !output.status.success() {
            bail!(
                "imb status: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        parse_status(&String::from_utf8_lossy(&output.stdout))
    }

    /// The mirror for this iPhone; imb stays running while it is open and the
    /// mirror quits when imb ends, so killing the child closes the window.
    /// `height` is the window height in points, as for scrcpy; 0 or an empty
    /// title leaves imb's default; `touch_pointer` off keeps macOS's pointer.
    pub fn spawn_mirror(
        &self,
        udid: &str,
        height: u32,
        title: &str,
        touch_pointer: bool,
    ) -> Result<Child> {
        let mut command = Command::new(&self.path);
        command.args(["mirror", "--wait", "--udid", udid]);
        add_window_args(&mut command, height, touch_pointer);
        if !title.is_empty() {
            command.args(["--title", title]);
        }
        Self::spawn(command)
    }

    /// Android v iOS: this Android phone and this iPhone side by side in one
    /// window, `gap` points apart, through imb's own scrcpy client.
    pub fn spawn_pair(&self, serial: &str, udid: &str, options: &PairOptions) -> Result<Child> {
        let mut command = Command::new(&self.path);
        command.args(["devices", "--wait", "--serial", serial, "--udid", udid]);
        command.args(["--gap", &options.gap.to_string()]);
        add_window_args(&mut command, options.height, options.touch_pointer);
        if !options.android_title.is_empty() {
            command.args(["--android-title", &options.android_title]);
        }
        if !options.iphone_title.is_empty() {
            command.args(["--iphone-title", &options.iphone_title]);
        }
        Self::spawn(command)
    }

    fn spawn(mut command: Command) -> Result<Child> {
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("starting imb")
    }
}

/// The window of Android v iOS, as awb's Settings set it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairOptions {
    pub height: u32,
    pub gap: u32,
    pub android_title: String,
    pub iphone_title: String,
    pub touch_pointer: bool,
}

fn add_window_args(command: &mut Command, height: u32, touch_pointer: bool) {
    if height > 0 {
        command.args(["--height", &height.to_string()]);
    }
    if !touch_pointer {
        command.args(["--cursor", "system"]);
    }
}

pub fn parse_status(json: &str) -> Result<Vec<Iphone>> {
    Ok(serde_json::from_str::<Status>(json)
        .context("reading imb status")?
        .devices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_imb_status() {
        let json = r#"{"devices": [{"mirroring": false, "model": "iPhone Air",
            "name": "Test iPhone", "productType": "iPhone18,4", "udid": "00008150-AAAA"}]}"#;
        assert_eq!(
            parse_status(json).unwrap(),
            vec![Iphone {
                udid: "00008150-AAAA".into(),
                name: "Test iPhone".into(),
                model: "iPhone Air".into(),
            }]
        );
    }
}
