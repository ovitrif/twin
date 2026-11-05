//! dmb-hid: live touch, typing and buttons for one iPhone over USB, through
//! the CoreDevice HID service Xcode's Device Hub uses (iOS 27 and later).
//! Nothing is installed on the phone.
//!
//! Usage: `dmb-hid <udid> [--video]`, then one JSON command per line on stdin:
//!   {"touch":"down"|"move"|"up","x":0.5,"y":0.25}   x and y from 0 to 1 across the screen
//!   {"text":"hello"}
//!   {"key":79}                                     a HID keyboard usage (79 is right arrow)
//!   {"button":"home"|"lock"|"volume-up"|"volume-down"|"siri"}
//!   {"keyframe":true}                              with --video: ask for a full frame
//! It prints `ready` once the phone accepts input, and `error: <why>` lines.
//!
//! With --video it also streams the phone's screen: stdout carries H.265
//! access units, each a 4-byte big-endian length and Annex B NAL units, and
//! the text lines above go to stderr, after a first `screen <width> <height>`
//! line with the screen's size in pixels (the stream pads it to a multiple of
//! 16). This is the Device Hub stream, which leaves the phone's status bar
//! alone, unlike the QuickTime capture path.
//!
//! `dmb-hid <udid> --ui` prints the front app's elements as one JSON array and
//! exits (see ui.rs).

mod stream;
mod ui;

use idevice::{
    IdeviceService, RsdService,
    core_device::{
        ButtonState, IndigoHidClient, TOUCHSCREEN_STATE_CONTACT, TOUCHSCREEN_STATE_RELEASE,
        UniversalHidServiceClient,
    },
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
    keyframe: Option<bool>,
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
        '!' => (0x1E, true),
        '@' => (0x1F, true),
        '#' => (0x20, true),
        '$' => (0x21, true),
        '%' => (0x22, true),
        '^' => (0x23, true),
        '&' => (0x24, true),
        '*' => (0x25, true),
        '(' => (0x26, true),
        ')' => (0x27, true),
        '-' => (0x2D, false),
        '_' => (0x2D, true),
        '=' => (0x2E, false),
        '+' => (0x2E, true),
        '[' => (0x2F, false),
        '{' => (0x2F, true),
        ']' => (0x30, false),
        '}' => (0x30, true),
        '\\' => (0x31, false),
        '|' => (0x31, true),
        ';' => (0x33, false),
        ':' => (0x33, true),
        '\'' => (0x34, false),
        '"' => (0x34, true),
        '`' => (0x35, false),
        '~' => (0x35, true),
        ',' => (0x36, false),
        '<' => (0x36, true),
        '.' => (0x37, false),
        '>' => (0x37, true),
        '/' => (0x38, false),
        '?' => (0x38, true),
        _ => return None,
    })
}

/// 0 to 1 across the screen as the HID's 0 to 65535.
fn scaled(v: f64) -> u16 {
    (v.clamp(0.0, 1.0) * 65535.0).round() as u16
}

/// Status lines go to stdout, or to stderr when stdout carries video
static VIDEO: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn say(line: &str) {
    if VIDEO.load(std::sync::atomic::Ordering::Relaxed) {
        eprintln!("{line}");
    } else {
        println!("{line}");
    }
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(udid) = args.first().filter(|a| !a.starts_with("--")).cloned() else {
        eprintln!("usage: dmb-hid <udid> [--video | --ui]");
        std::process::exit(2);
    };
    if args.iter().any(|a| a == "--ui") {
        if let Err(e) = inspect(&udid).await {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
        return;
    }
    VIDEO.store(
        args.iter().any(|a| a == "--video"),
        std::sync::atomic::Ordering::Relaxed,
    );
    if let Err(e) = run(&udid).await {
        say(&format!("error: {e}"));
        std::process::exit(1);
    }
}

/// The screen's size in pixels, read from one screenshot's PNG header
async fn screen_size(
    adapter: &mut idevice::tcp::handle::AdapterHandle,
    handshake: &mut RsdHandshake,
) -> Result<(u32, u32), String> {
    let mut server =
        idevice::dvt::remote_server::RemoteServerClient::connect_rsd(adapter, handshake)
            .await
            .map_err(|e| format!("instruments: {e:?}"))?;
    server
        .read_message(0)
        .await
        .map_err(|e| format!("instruments: {e:?}"))?;
    let mut shots = idevice::dvt::screenshot::ScreenshotClient::new(&mut server)
        .await
        .map_err(|e| format!("screenshot service: {e:?}"))?;
    let png = shots
        .take_screenshot()
        .await
        .map_err(|e| format!("screenshot: {e:?}"))?;
    if png.len() < 24 || &png[1..4] != b"PNG" {
        return Err("screenshot is not a PNG".into());
    }
    let be = |i: usize| u32::from_be_bytes([png[i], png[i + 1], png[i + 2], png[i + 3]]);
    Ok((be(16), be(20)))
}

/// What the video reader learned, for the RTCP we send back
#[derive(Default)]
struct VideoPeer {
    port: u16,
    ssrc: u32,
    highest_seq: u32,
    fir_seq: u8,
}

fn report_block(peer: &VideoPeer) -> idevice::core_device::ReportBlock {
    idevice::core_device::ReportBlock {
        source_ssrc: peer.ssrc,
        fraction_lost: 0,
        cumulative_lost: 0,
        highest_seq: peer.highest_seq,
        jitter: 0,
        lsr: 0,
        dlsr: 0,
    }
}

/// A CoreDevice tunnel to the phone and its service list
async fn tunnel(udid: &str) -> Result<(idevice::tcp::handle::AdapterHandle, RsdHandshake), String> {
    let mut usbmuxd = UsbmuxdConnection::default()
        .await
        .map_err(|e| format!("usbmuxd: {e:?}"))?;
    let device = usbmuxd
        .get_device(udid)
        .await
        .map_err(|e| format!("no iPhone {udid} on USB: {e:?}"))?;
    let provider = device.to_provider(UsbmuxdAddr::default(), "dmb");
    let proxy = CoreDeviceProxy::connect(&provider)
        .await
        .map_err(|e| format!("CoreDevice: {e:?}"))?;
    let rsd_port = proxy.tunnel_info().server_rsd_port;
    let mut adapter = proxy
        .create_software_tunnel()
        .map_err(|e| format!("tunnel: {e:?}"))?
        .to_async_handle();
    let rsd = adapter
        .connect(rsd_port)
        .await
        .map_err(|e| format!("RSD: {e:?}"))?;
    let handshake = RsdHandshake::new(rsd)
        .await
        .map_err(|e| format!("RSD handshake: {e:?}"))?;
    Ok((adapter, handshake))
}

/// `--ui`: one walk through the front app's elements, printed as JSON
async fn inspect(udid: &str) -> Result<(), String> {
    let (mut adapter, handshake) = tunnel(udid).await?;
    let mut inspector = ui::Inspector::connect(&mut adapter, &handshake).await?;
    let result = inspector.elements().await;
    inspector.finish().await;
    println!(
        "{}",
        serde_json::to_string(&result?).map_err(|e| e.to_string())?
    );
    Ok(())
}

async fn run(udid: &str) -> Result<(), String> {
    let (mut adapter, mut handshake) = tunnel(udid).await?;

    let video_mode = VIDEO.load(std::sync::atomic::Ordering::Relaxed);
    if video_mode {
        let (w, h) = screen_size(&mut adapter, &mut handshake).await?;
        say(&format!("screen {w} {h}"));
    }
    let media = stream::start(&mut adapter, &mut handshake).await?;
    let our_ssrc = media.ssrc;
    let (audio, video) = (media.audio_udp, std::sync::Arc::new(media.video_udp));
    let peer = std::sync::Arc::new(tokio::sync::Mutex::new(VideoPeer::default()));
    // The stream must exist for touch to work; its audio is always dropped,
    // its video too unless --video
    tokio::spawn(async move { while audio.recv().await.is_ok() {} });
    if video_mode {
        spawn_video(video.clone(), peer.clone(), our_ssrc);
    } else {
        let video = video.clone();
        tokio::spawn(async move { while video.recv().await.is_ok() {} });
    }
    // The phone needs a moment to match its HID surfaces to the new stream
    tokio::time::sleep(Duration::from_millis(300)).await;

    let mut touch = UniversalHidServiceClient::connect_rsd(&mut adapter, &mut handshake)
        .await
        .map_err(|e| format!("touch service: {e:?}"))?;
    let mut indigo = IndigoHidClient::connect_rsd(&mut adapter, &mut handshake)
        .await
        .map_err(|e| format!("button service: {e:?}"))?;
    say("ready");

    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut last = (0u16, 0u16);
    while let Ok(Some(line)) = lines.next_line().await {
        let Ok(command) = serde_json::from_str::<Command>(&line) else {
            say(&format!("error: bad command {line}"));
            continue;
        };
        let result = if let Some(phase) = command.touch.as_deref() {
            let point = match (command.x, command.y) {
                (Some(x), Some(y)) => (scaled(x), scaled(y)),
                _ => last,
            };
            last = point;
            let state = if phase == "up" {
                TOUCHSCREEN_STATE_RELEASE
            } else {
                TOUCHSCREEN_STATE_CONTACT
            };
            touch
                .send_touchscreen(state, point.0, point.1, None)
                .await
                .map_err(|e| format!("{e:?}"))
        } else if command.keyframe == Some(true) {
            request_keyframe(&video, &peer, our_ssrc).await
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
            say(&format!("error: {e}"));
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
    indigo
        .send_button(0x0C, usage, ButtonState::Down)
        .await
        .map_err(|e| format!("{e:?}"))?;
    tokio::time::sleep(Duration::from_millis(hold)).await;
    indigo
        .send_button(0x0C, usage, ButtonState::Up)
        .await
        .map_err(|e| format!("{e:?}"))
}

async fn press_key(
    indigo: &mut IndigoHidClient<Box<dyn idevice::ReadWrite>>,
    usage: u64,
) -> Result<(), String> {
    indigo
        .send_keyboard(usage, ButtonState::Down)
        .await
        .map_err(|e| format!("{e:?}"))?;
    indigo
        .send_keyboard(usage, ButtonState::Up)
        .await
        .map_err(|e| format!("{e:?}"))
}

async fn type_text(
    indigo: &mut IndigoHidClient<Box<dyn idevice::ReadWrite>>,
    text: &str,
) -> Result<(), String> {
    const SHIFT: u64 = 0xE1;
    for c in text.chars() {
        let Some((usage, shift)) = key(c) else {
            continue;
        };
        if shift {
            indigo
                .send_keyboard(SHIFT, ButtonState::Down)
                .await
                .map_err(|e| format!("{e:?}"))?;
        }
        indigo
            .send_keyboard(usage, ButtonState::Down)
            .await
            .map_err(|e| format!("{e:?}"))?;
        indigo
            .send_keyboard(usage, ButtonState::Up)
            .await
            .map_err(|e| format!("{e:?}"))?;
        if shift {
            indigo
                .send_keyboard(SHIFT, ButtonState::Up)
                .await
                .map_err(|e| format!("{e:?}"))?;
        }
    }
    Ok(())
}

/// Asks the phone for a full frame, as after a gap or when the view starts
async fn request_keyframe(
    video: &idevice::tcp::handle::UdpSocketHandle,
    peer: &tokio::sync::Mutex<VideoPeer>,
    our_ssrc: u32,
) -> Result<(), String> {
    let mut p = peer.lock().await;
    if p.port == 0 {
        return Ok(()); // no packet yet; the stream starts with a key frame anyway
    }
    p.fir_seq = p.fir_seq.wrapping_add(1);
    let packet = idevice::core_device::build_keyframe_request(
        our_ssrc,
        "dmb",
        p.ssrc,
        &[report_block(&p)],
        p.fir_seq,
    );
    let port = p.port;
    drop(p);
    video
        .send_to(port, packet)
        .await
        .map_err(|e| format!("keyframe request: {e:?}"))
}

/// Reads the video's RTP, writes whole access units to stdout and keeps the
/// phone told that we are listening
fn spawn_video(
    video: std::sync::Arc<idevice::tcp::handle::UdpSocketHandle>,
    peer: std::sync::Arc<tokio::sync::Mutex<VideoPeer>>,
    our_ssrc: u32,
) {
    use std::io::Write;
    let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        let mut out = std::io::BufWriter::new(std::io::stdout());
        while let Ok(frame) = rx.recv() {
            let length = (frame.len() as u32).to_be_bytes();
            if out
                .write_all(&length)
                .and_then(|_| out.write_all(&frame))
                .and_then(|_| out.flush())
                .is_err()
            {
                std::process::exit(0); // the viewer went away
            }
        }
    });
    let live = (video.clone(), peer.clone());
    tokio::spawn(async move {
        let (video, peer) = live;
        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;
            let p = peer.lock().await;
            if p.port != 0 {
                let packet =
                    idevice::core_device::build_liveness(our_ssrc, "dmb", &[report_block(&p)]);
                let port = p.port;
                drop(p);
                let _ = video.send_to(port, packet).await;
            }
        }
    });
    tokio::spawn(async move {
        let mut depacketizer = idevice::core_device::HevcDepacketizer::new();
        let mut frame: Vec<u8> = Vec::new();
        let mut frame_ts: Option<u32> = None;
        let mut asked = false;
        while let Ok(datagram) = video.recv().await {
            if idevice::core_device::is_rtcp(&datagram.data) {
                continue;
            }
            let Some(packet) = idevice::core_device::RtpPacket::parse(&datagram.data) else {
                continue;
            };
            {
                let mut p = peer.lock().await;
                p.port = datagram.source_port;
                p.ssrc = packet.ssrc;
                p.highest_seq = packet.sequence_number as u32;
            }
            if !asked {
                asked = true;
                let _ = request_keyframe(&video, &peer, our_ssrc).await;
            }
            // A new timestamp starts the next access unit
            if frame_ts.is_some_and(|ts| ts != packet.timestamp) && !frame.is_empty() {
                let _ = tx.send(std::mem::take(&mut frame));
            }
            frame_ts = Some(packet.timestamp);
            depacketizer.push(packet.sequence_number, packet.timestamp, packet.payload);
            frame.extend(depacketizer.take_output());
            if packet.marker && !frame.is_empty() {
                let _ = tx.send(std::mem::take(&mut frame));
            }
        }
    });
}
