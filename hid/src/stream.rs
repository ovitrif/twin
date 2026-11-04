// Starting the displayservice screen media stream, after idevice's
// `tools/src/coredevice_stream.rs` (MIT, Jackson Coxson). The device drops
// synthetic HID events unless such a stream is active, so dmb-hid holds one
// open for as long as it runs and discards its packets.

use idevice::{
    ReadWrite, RsdService,
    core_device::{
        CallInfoBlob, DisplayServiceClient, build_screen_audio_offer, build_screen_video_offer,
        build_start_audio_parameters, build_start_video_parameters,
    },
    rsd::RsdHandshake,
    tcp::handle::{AdapterHandle, UdpSocketHandle},
};

/// What we (the controller) support; Device Hub sends 140 for screen sharing.
const CLIENT_SUPPORTED_FEATURES: u64 = 140;

pub struct ScreenMediaStream {
    pub client: DisplayServiceClient<Box<dyn ReadWrite>>,
    pub audio_udp: UdpSocketHandle,
    pub video_udp: UdpSocketHandle,
}

pub async fn start(
    adapter: &mut AdapterHandle,
    handshake: &mut RsdHandshake,
) -> Result<ScreenMediaStream, String> {
    let mut client = DisplayServiceClient::connect_rsd(adapter, handshake)
        .await
        .map_err(|e| format!("no display service: {e:?}"))?;
    let audio_udp = adapter.bind_udp(0).await.map_err(|e| format!("bind audio: {e:?}"))?;
    let video_udp = adapter.bind_udp(0).await.map_err(|e| format!("bind video: {e:?}"))?;
    let receiver_ip = adapter.host_ip().to_string();
    let sender_ip = adapter.peer_ip().to_string();
    let call_info = CallInfoBlob {
        call_id: 0,
        client_version: 1,
        device_type: "Mac17,7".into(),
        framework_version: "2205.3.1".into(),
        os_version: "25F71".into(),
        device_name: None,
        audio_device_uid: None,
    };
    // Audio first establishes the screen-sharing session; video joins it
    let session = uuid::Uuid::new_v4();
    let audio_call = uuid::Uuid::new_v4().to_string().to_uppercase();
    let audio_offer = build_screen_audio_offer(&audio_call, &call_info)
        .map_err(|e| format!("audio offer: {e:?}"))?;
    client
        .start_media_stream(build_start_audio_parameters(
            &receiver_ip, audio_udp.local_port(), &sender_ip, 50000, audio_offer,
            CLIENT_SUPPORTED_FEATURES, session,
        ))
        .await
        .map_err(|e| format!("audio stream: {e:?}"))?;
    let video_call = uuid::Uuid::new_v4().to_string().to_uppercase();
    let ssrc = uuid::Uuid::new_v4().as_u128() as u32;
    let video_offer = build_screen_video_offer(&video_call, &call_info, ssrc)
        .map_err(|e| format!("video offer: {e:?}"))?;
    client
        .start_media_stream(build_start_video_parameters(
            &receiver_ip, video_udp.local_port(), &sender_ip, 50001, video_offer,
            CLIENT_SUPPORTED_FEATURES, 1, session,
        ))
        .await
        .map_err(|e| format!("video stream: {e:?}"))?;
    Ok(ScreenMediaStream { client, audio_udp, video_udp })
}
