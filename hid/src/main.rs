//! imb-hid: live touch, typing and buttons for one iPhone over USB, through
//! the CoreDevice HID service Xcode's Device Hub uses (iOS 27 and later).
//! Nothing is installed on the phone.
//!
//! Usage: `imb-hid <udid>`, then one JSON command per line on stdin:
//!   {"touch":"down"|"move"|"up","x":0.5,"y":0.25}   x and y from 0 to 1 across the screen
//!   {"text":"hello"}
//!   {"key":79}                                     a HID keyboard usage (79 is right arrow)
//!   {"button":"home"|"lock"|"volume-up"|"volume-down"|"siri"}
//! It prints `ready` once the phone accepts input, and `error: <why>` lines.

mod stream;

use idevice::{
    IdeviceService, RsdService,
    core_device::{ButtonState, IndigoHidClient, TOUCHSCREEN_STATE_CONTACT, TOUCHSCREEN_STATE_RELEASE,
                  UniversalHidServiceClient},
    core_device_proxy::CoreDeviceProxy,
    rsd::RsdHandshake,
    usbmuxd::{UsbmuxdAddr, UsbmuxdConnection},
};
use serde::Deserialize;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};

#[derive(Deserialize)]
struct Command {
    touch: Option<String>,
    x: Option<f64>,
    y: Option<f64>,
    text: Option<String>,
    key: Option<u64>,
    button: Option<String>,
}

/// Named buttons: consumer usage page 0x0C, usage, hold in ms.
fn button(name: &str) -> Option<(u64, u64)> {
    Some(match name {
        "home" => (0x40, 80),
        "lock" => (0x30, 200),
        "volume-up" => (0xE9, 80),
        "volume-down" => (0xEA, 80),
        "siri" => (0xCF, 1200),
        _ => return None,
    })
}

/// HID keyboard usage for a character, and whether it needs Shift.
fn key(c: char) -> Option<(u64, bool)> {
    Some(match c {
        'a'..='z' => (0x04 + (c as u64 - 'a' as u64), false),
        'A'..='Z' => (0x04 + (c as u64 - 'A' as u64), true),
        '1'..='9' => (0x1E + (c as u64 - '1' as u64), false),
        '0' => (0x27, false),
        '\n' => (0x28, false),
        '\u{8}' => (0x2A, false), // backspace
        '\t' => (0x2B, false),
        ' ' => (0x2C, false),
        '\u{7F}' => (0x4C, false), // forward delete
        '!' => (0x1E, true), '@' => (0x1F, true), '#' => (0x20, true), '$' => (0x21, true),
        '%' => (0x22, true), '^' => (0x23, true), '&' => (0x24, true), '*' => (0x25, true),
        '(' => (0x26, true), ')' => (0x27, true), '-' => (0x2D, false), '_' => (0x2D, true),
        '=' => (0x2E, false), '+' => (0x2E, true), '[' => (0x2F, false), '{' => (0x2F, true),
        ']' => (0x30, false), '}' => (0x30, true), '\\' => (0x31, false), '|' => (0x31, true),
        ';' => (0x33, false), ':' => (0x33, true), '\'' => (0x34, false), '"' => (0x34, true),
        '`' => (0x35, false), '~' => (0x35, true), ',' => (0x36, false), '<' => (0x36, true),
        '.' => (0x37, false), '>' => (0x37, true), '/' => (0x38, false), '?' => (0x38, true),
        _ => return None,
    })
}

/// 0 to 1 across the screen as the HID's 0 to 65535.
fn scaled(v: f64) -> u16 {
    (v.clamp(0.0, 1.0) * 65535.0).round() as u16
}

#[tokio::main]
async fn main() {
    let Some(udid) = std::env::args().nth(1) else {
        eprintln!("usage: imb-hid <udid>");
        std::process::exit(2);
    };
    if let Err(e) = run(&udid).await {
        println!("error: {e}");
        std::process::exit(1);
    }
}

async fn run(udid: &str) -> Result<(), String> {
    let mut usbmuxd = UsbmuxdConnection::default().await.map_err(|e| format!("usbmuxd: {e:?}"))?;
    let device = usbmuxd.get_device(udid).await.map_err(|e| format!("no iPhone {udid} on USB: {e:?}"))?;
    let provider = device.to_provider(UsbmuxdAddr::default(), "imb");
    let proxy = CoreDeviceProxy::connect(&provider).await.map_err(|e| format!("CoreDevice: {e:?}"))?;
    let rsd_port = proxy.tunnel_info().server_rsd_port;
    let mut adapter = proxy.create_software_tunnel().map_err(|e| format!("tunnel: {e:?}"))?.to_async_handle();
    let rsd = adapter.connect(rsd_port).await.map_err(|e| format!("RSD: {e:?}"))?;
    let mut handshake = RsdHandshake::new(rsd).await.map_err(|e| format!("RSD handshake: {e:?}"))?;

    let media = stream::start(&mut adapter, &mut handshake).await?;
    // Drain the stream's packets so they do not pile up in the tunnel
    let (audio, video) = (media.audio_udp, media.video_udp);
    tokio::spawn(async move { while audio.recv().await.is_ok() {} });
    tokio::spawn(async move { while video.recv().await.is_ok() {} });
    // The phone needs a moment to match its HID surfaces to the new stream
    tokio::time::sleep(Duration::from_millis(300)).await;

    let mut touch = UniversalHidServiceClient::connect_rsd(&mut adapter, &mut handshake)
        .await
        .map_err(|e| format!("touch service: {e:?}"))?;
    let mut indigo = IndigoHidClient::connect_rsd(&mut adapter, &mut handshake)
        .await
        .map_err(|e| format!("button service: {e:?}"))?;
    println!("ready");

    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut last = (0u16, 0u16);
    while let Ok(Some(line)) = lines.next_line().await {
        let Ok(command) = serde_json::from_str::<Command>(&line) else {
            println!("error: bad command {line}");
            continue;
        };
        let result = if let Some(phase) = command.touch.as_deref() {
            let point = match (command.x, command.y) {
                (Some(x), Some(y)) => (scaled(x), scaled(y)),
                _ => last,
            };
            last = point;
            let state = if phase == "up" { TOUCHSCREEN_STATE_RELEASE } else { TOUCHSCREEN_STATE_CONTACT };
            touch.send_touchscreen(state, point.0, point.1, None).await.map_err(|e| format!("{e:?}"))
        } else if let Some(text) = command.text {
            type_text(&mut indigo, &text).await
        } else if let Some(usage) = command.key {
            press_key(&mut indigo, usage).await
        } else if let Some((usage, hold)) = command.button.as_deref().and_then(button) {
            press(&mut indigo, usage, hold).await
        } else {
            Err(format!("unknown command {line}"))
        };
        if let Err(e) = result {
            println!("error: {e}");
            // A dropped tunnel (the phone replugged or switched USB modes) does
            // not come back; exit so the mirror starts a fresh session
            if e.contains("BrokenPipe") || e.contains("channel closed") {
                std::process::exit(3);
            }
        }
    }
    let mut client = media.client;
    let _ = client.stop_media_stream().await;
    Ok(())
}

async fn press(
    indigo: &mut IndigoHidClient<Box<dyn idevice::ReadWrite>>,
    usage: u64,
    hold: u64,
) -> Result<(), String> {
    indigo.send_button(0x0C, usage, ButtonState::Down).await.map_err(|e| format!("{e:?}"))?;
    tokio::time::sleep(Duration::from_millis(hold)).await;
    indigo.send_button(0x0C, usage, ButtonState::Up).await.map_err(|e| format!("{e:?}"))
}

async fn press_key(indigo: &mut IndigoHidClient<Box<dyn idevice::ReadWrite>>, usage: u64) -> Result<(), String> {
    indigo.send_keyboard(usage, ButtonState::Down).await.map_err(|e| format!("{e:?}"))?;
    indigo.send_keyboard(usage, ButtonState::Up).await.map_err(|e| format!("{e:?}"))
}

async fn type_text(indigo: &mut IndigoHidClient<Box<dyn idevice::ReadWrite>>, text: &str) -> Result<(), String> {
    const SHIFT: u64 = 0xE1;
    for c in text.chars() {
        let Some((usage, shift)) = key(c) else { continue };
        if shift {
            indigo.send_keyboard(SHIFT, ButtonState::Down).await.map_err(|e| format!("{e:?}"))?;
        }
        indigo.send_keyboard(usage, ButtonState::Down).await.map_err(|e| format!("{e:?}"))?;
        indigo.send_keyboard(usage, ButtonState::Up).await.map_err(|e| format!("{e:?}"))?;
        if shift {
            indigo.send_keyboard(SHIFT, ButtonState::Up).await.map_err(|e| format!("{e:?}"))?;
        }
    }
    Ok(())
}
