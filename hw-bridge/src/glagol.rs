//! Stateful `glagold` mock for `Stub Mode` covering both the internal Quasar IPC socket
//! (`127.0.0.1:9899`) and the external Glagol HTTP / WebSocket protocol port (`0.0.0.0:1961`).
//!
//! ### 1. Internal `glagold` IPC Protocol (`127.0.0.1:9899`)
//!
//! All local Quasar daemons (`maind --glagol-connections` invoked by `/system/vendor/quasar/q glagol-connections`,
//! `metrics_collector`, `GlagolDiscoveryProxy`) communicate with `glagold` on `127.0.0.1:9899` using
//! length-prefixed `QuasarMessage` frames (`[u32_le len][QuasarMessage protobuf]`).
//!
//! Disassembly of `maind --glagol-connections` (`0x77c2a4..0x77de72`) revealed a non-obvious
//! two-step handshake:
//! 1. `--glagol-connections` connects to `127.0.0.1:9899` and sends `QuasarMessage` with
//!    `type = 185` (`TYPE_GLAGOL_DISCOVERY_REQUEST = 185`) containing
//!    `GlagolDiscoveryRequest { debug_connections = 8 (EmptyMessage) }`:
//!    - Legacy firmware (`<= 0.367`, e.g. `1.266.4.40`, `0.13.1.36`):
//!      `05 00 00 00 ca 0b 02 42 00` (direct field `185`, wire tag `0xca 0x0b`).
//!    - Modern `LITE_RUNTIME` firmware (`>= 0.368`, e.g. `0.369.1.*`, `0.373.1.13`):
//!      `09 00 00 00 e2 0f 02 42 00 e8 0f b9 01` (`payload = 252`, `type = 253 (185)`).
//! 2. At `0x77dd28` (`movs r2, #0xb8; bl #0x99706a`), `--glagol-connections` waits on a
//!    `std::future<std::string>` specifically for a `QuasarMessage` of type **`184`
//!    (`0xb8 = TYPE_LOG_MESSAGE`)** containing `quasar.proto.LogMessage { optional string msg = 4; }`.
//!    It ignores any message whose type is not `184`, parses `log_message.msg()` as JSON via
//!    `Json::CharReader::parse` (`0x77c38a`), pretty-prints the JSON to `stdout`, and exits with
//!    code `0`.
//! 3. Meanwhile, `GlagolDiscoveryProxy` and `metrics_collector` expect `GlagolDiscoveryResult`
//!    (`type = 187`, with `items = 1` and `is_full_result = 3 {}`) and `GlagoldState` (`type = 82`,
//!    with `connections = 1`, `cluster_device_list = 7`, `public_ip = 8`).
//!
//! Replying to `GlagolDiscoveryRequest` (`185`) with the atomic 3-frame bundle
//! `[GlagolDiscoveryResult (187), GlagoldState (82), LogMessage (184)]` encoded in the peer's
//! negotiated `QuasarWireFormat` (`LegacyDirect` vs `Envelope252`) satisfies both
//! `q glagol-connections` and `metrics_collector` / `GlagolDiscoveryProxy`.
//!
//! ### 2. External Glagol Endpoint (`0.0.0.0:1961`)
//!
//! External automation scripts and local Glagol clients connect to port `:1961` via HTTP
//! (`GET /`, `GET /state`, `POST /command`) or RFC 6455 WebSocket (`Upgrade: websocket`).
//! The endpoint exposes a live speaker state snapshot (`id: "EMU0001"`, `platform`, `aliceState`,
//! `volume`, `volumeLevel`, `micsMuted`, `playing`, `playerState`, `connections`) and executes
//! playback (`play`, `pause`, `stop`, `prev`, `next`), volume (`setVolume` via `BridgeState` +
//! ALSA `/system/vendor/quasar/set_volume.sh`), and voice/text (`sendText` / `vins` routed through
//! `voice_dialog::handle_inject_voice`) commands while recording active peers in `BridgeState`
//! so they appear in `:9899` discovery and `q glagol-connections`.

use crate::proto::{
    build_glagol_discovery_result_packet, build_glagol_log_message_packet,
    build_glagold_state_packet, decode_quasar_glagol_packet_with_format, DecodedGlagolIpcPacket,
    GlagolDiscoveryItem, GlagolDiscoveryResult, GlagoldState, QuasarWireFormat,
};
use crate::state::{now_millis, AliceState, BridgeState};
use crate::voice_dialog::{handle_inject_voice, set_host_volume};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::Command;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

const DEVICE_ID: &str = "EMU0001";
const DEFAULT_PUBLIC_IP: &str = "10.0.2.15";
const WS_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

/// Represents an external or local Glagol client connection tracked by `BridgeState`
/// and reflected in both `:1961` state snapshots and `:9899` `glagold` IPC responses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlagolPeerConnection {
    pub peer_id: String,
    pub address: String,
    pub protocol: String,
    pub connected_at_ms: u64,
    pub last_command: Option<String>,
    pub active: bool,
}

impl GlagolPeerConnection {
    pub fn connection_descriptor(&self) -> String {
        format!("{}://{} ({})", self.protocol, self.address, self.peer_id)
    }

    pub fn to_json(&self) -> Value {
        json!({
            "id": self.peer_id,
            "address": self.address,
            "protocol": self.protocol,
            "connectedAtMs": self.connected_at_ms,
            "lastCommand": self.last_command,
            "active": self.active,
        })
    }
}

/// Registers or updates a Glagol client peer in `BridgeState::glagol_peers`.
pub fn upsert_glagol_peer(
    state: &BridgeState,
    peer_id: &str,
    address: &str,
    protocol: &str,
    last_command: Option<String>,
    active: bool,
) {
    let mut peers = state.glagol_peers.lock().unwrap();
    if let Some(existing) = peers
        .iter_mut()
        .find(|p| p.peer_id == peer_id || p.address == address)
    {
        existing.protocol = protocol.to_string();
        if last_command.is_some() {
            existing.last_command = last_command;
        }
        existing.active = active;
        return;
    }
    if peers.len() >= 16 {
        peers.remove(0);
    }
    peers.push(GlagolPeerConnection {
        peer_id: peer_id.to_string(),
        address: address.to_string(),
        protocol: protocol.to_string(),
        connected_at_ms: now_millis(),
        last_command,
        active,
    });
}

/// Builds the `GlagolDiscoveryResult` (`type = 187`) protobuf model from current `BridgeState`.
pub fn build_discovery_result_model(state: &BridgeState) -> GlagolDiscoveryResult {
    let mut items = Vec::new();
    items.push(GlagolDiscoveryItem {
        device_id: Some(DEVICE_ID.to_string()),
        platform: Some(state.board_slug.clone()),
        name: Some(state.board_name.clone()),
        address: Some("127.0.0.1:1961".to_string()),
        is_local_connection_allowed: Some(true),
    });
    let peers = state.glagol_peers.lock().unwrap();
    for peer in peers.iter() {
        items.push(GlagolDiscoveryItem {
            device_id: Some(peer.peer_id.clone()),
            platform: Some("glagol_client".to_string()),
            name: Some(format!("Glagol Peer ({})", peer.protocol)),
            address: Some(peer.address.clone()),
            is_local_connection_allowed: Some(true),
        });
    }
    GlagolDiscoveryResult {
        items,
        removed_items: Vec::new(),
        is_full_result: true,
    }
}

/// Builds the `GlagoldState` (`type = 82`) protobuf model from current `BridgeState`.
pub fn build_glagold_state_model(state: &BridgeState) -> GlagoldState {
    let peers = state.glagol_peers.lock().unwrap();
    let mut connections: Vec<String> = peers.iter().map(|p| p.connection_descriptor()).collect();
    if connections.is_empty() {
        connections.push("local://127.0.0.1:1961 (EMU0001)".to_string());
    }
    let mut cluster_device_list = vec![DEVICE_ID.to_string()];
    for peer in peers.iter() {
        if !cluster_device_list.contains(&peer.peer_id) {
            cluster_device_list.push(peer.peer_id.clone());
        }
    }
    GlagoldState {
        connections,
        cluster_device_list,
        public_ip: Some(DEFAULT_PUBLIC_IP.to_string()),
    }
}

/// Builds the JSON string carried inside `LogMessage.msg` (`type = 184`, field `4`) for
/// `/system/vendor/quasar/q glagol-connections` (`maind --glagol-connections`) on `:9899`.
pub fn build_glagol_connections_json(state: &BridgeState) -> String {
    let peers = state.glagol_peers.lock().unwrap();
    let peer_list: Vec<Value> = peers.iter().map(|p| p.to_json()).collect();
    let vol_level = state.volume_level.load(Ordering::Relaxed).min(10);
    let playing = state.player_playing.load(Ordering::Relaxed);
    let track_idx = state.player_track_index.load(Ordering::Relaxed).max(1);
    let payload = json!({
        "device_id": DEVICE_ID,
        "platform": state.board_slug,
        "name": state.board_name,
        "public_ip": DEFAULT_PUBLIC_IP,
        "glagol_port": 1961,
        "alice_state": state.alice_state().as_str(),
        "volume_level": vol_level,
        "playing": playing,
        "track_index": track_idx,
        "cluster_device_list": [DEVICE_ID],
        "connections": peer_list,
    });
    serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string())
}

/// Builds the complete 3-frame reply bundle (`GlagolDiscoveryResult` `187`, `GlagoldState` `82`,
/// and `LogMessage` `184`) in `wire_format` for `:9899` `glagold` IPC clients.
pub fn build_glagold_ipc_reply_bundle(
    state: &BridgeState,
    wire_format: QuasarWireFormat,
) -> Vec<u8> {
    let disc = build_discovery_result_model(state);
    let gstate = build_glagold_state_model(state);
    let log_json = build_glagol_connections_json(state);

    let mut bundle = build_glagol_discovery_result_packet(&disc, wire_format);
    bundle.extend_from_slice(&build_glagold_state_packet(&gstate, wire_format));
    bundle.extend_from_slice(&build_glagol_log_message_packet(&log_json, wire_format));
    bundle
}

/// Decodes a single unframed `QuasarMessage` payload received on `127.0.0.1:9899` (`glagold`)
/// and returns the framed response bundle (`187` + `82` + `184`) in the peer's wire format.
pub fn handle_glagold_ipc_payload(
    state: &BridgeState,
    payload: &[u8],
) -> Result<(Vec<u8>, QuasarWireFormat), String> {
    let (decoded, peer_wf) = decode_quasar_glagol_packet_with_format(payload)?;
    state.set_wire_format(peer_wf);
    match decoded {
        DecodedGlagolIpcPacket::DiscoveryRequest(req) => {
            state.log(format!(
                "IPC :9899 (glagold) -> GlagolDiscoveryRequest (debug_connections={}, subscribe={}, wf={:?})",
                req.debug_connections, req.subscribe, peer_wf
            ));
        }
        DecodedGlagolIpcPacket::ClusterMessage(msg) => {
            state.log(format!(
                "IPC :9899 (glagold) -> GlagolClusterMessage (dst={:?})",
                msg.destination_device_id
            ));
        }
        DecodedGlagolIpcPacket::ServiceReady(svc) => {
            state.log(format!("IPC :9899 (glagold) -> service_name_ready: {}", svc));
        }
        DecodedGlagolIpcPacket::OtherQuasarMessage(type_id) => {
            state.log(format!("IPC :9899 (glagold) -> QuasarMessage type {}", type_id));
        }
    }
    let reply = build_glagold_ipc_reply_bundle(state, peer_wf);
    Ok((reply, peer_wf))
}

/// Handles a single TCP client stream on `127.0.0.1:9899` (`glagold`).
pub fn handle_glagold_ipc_stream(mut stream: TcpStream, state: Arc<BridgeState>) {
    let _ = stream.set_nodelay(true);
    let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));
    // Wait briefly for an initial request (such as `q glagol-connections` sending type=185).
    // If the client is a passive subscriber that sends nothing within 500ms, push an initial
    // GlagolDiscoveryResult (187) + GlagoldState (82) + LogMessage (184) snapshot in the firmware's wire format.
    let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
    let mut len_buf = [0u8; 4];
    match stream.read_exact(&mut len_buf) {
        Ok(()) => {
            let _ = stream.set_read_timeout(Some(Duration::from_millis(2000)));
            if !process_one_glagold_frame(&mut stream, &state, len_buf) {
                return;
            }
        }
        Err(ref e)
            if e.kind() == std::io::ErrorKind::WouldBlock
                || e.kind() == std::io::ErrorKind::TimedOut =>
        {
            let wf = state.wire_format();
            let init_pkt = build_glagold_ipc_reply_bundle(&state, wf);
            if stream.write_all(&init_pkt).is_err() {
                return;
            }
            let _ = stream.flush();
            let _ = stream.set_read_timeout(Some(Duration::from_millis(2000)));
        }
        Err(_) => return,
    }

    while stream.read_exact(&mut len_buf).is_ok() {
        if !process_one_glagold_frame(&mut stream, &state, len_buf) {
            break;
        }
    }
}

fn process_one_glagold_frame(
    stream: &mut TcpStream,
    state: &Arc<BridgeState>,
    len_buf: [u8; 4],
) -> bool {
    let len = u32::from_le_bytes(len_buf) as usize;
    if len == 0 || len > 65536 {
        return false;
    }
    let mut payload = vec![0u8; len];
    if stream.read_exact(&mut payload).is_err() {
        return false;
    }
    match handle_glagold_ipc_payload(state, &payload) {
        Ok((reply, _)) => {
            if stream.write_all(&reply).is_err() {
                return false;
            }
            let _ = stream.flush();
            true
        }
        Err(err) => {
            state.ipc_error_count.fetch_add(1, Ordering::SeqCst);
            state.log(format!(
                "ERROR IPC :9899 (glagold) -> Invalid Protobuf packet ({}B): {}",
                len, err
            ));
            false
        }
    }
}

/// Spawns the local `glagold` IPC server on `127.0.0.1:9899` in `Stub Mode`.
pub fn spawn_glagold_ipc_server(state: Arc<BridgeState>) {
    thread::spawn(move || {
        let Ok(listener) = TcpListener::bind("127.0.0.1:9899") else {
            state.log("Port 127.0.0.1:9899 already bound by native maind");
            return;
        };
        state.log(
            "Listening on 127.0.0.1:9899 (glagold stateful discovery & q glagol-connections IPC)",
        );
        for stream in listener.incoming().flatten() {
            let st = state.clone();
            thread::spawn(move || {
                handle_glagold_ipc_stream(stream, st);
            });
        }
    });
}

/// Builds the external Glagol speaker status JSON snapshot served on `:1961`.
pub fn build_glagol_state_json(state: &BridgeState) -> Value {
    let vol_level = state.volume_level.load(Ordering::Relaxed).min(10);
    let vol_float = (vol_level as f64) / 10.0;
    let playing = state.player_playing.load(Ordering::Relaxed);
    let track_idx = state.player_track_index.load(Ordering::Relaxed).max(1);
    let alice_state = state.alice_state().as_str();
    let muted = state.mics_muted.load(Ordering::Relaxed);
    let last_spoken = state.last_spoken_text.lock().unwrap().clone();
    let peers: Vec<Value> = state
        .glagol_peers
        .lock()
        .unwrap()
        .iter()
        .map(|p| p.to_json())
        .collect();

    let player_state = json!({
        "id": format!("track-{}", track_idx),
        "title": format!("Track #{}", track_idx),
        "subtitle": "Yandex Music",
        "hasPlay": !playing,
        "hasPause": playing,
        "hasNext": true,
        "hasPrev": true,
        "progress": if playing { 12.0 } else { 0.0 },
        "duration": 180.0,
        "type": "Track",
    });

    json!({
        "status": "ok",
        "id": DEVICE_ID,
        "platform": state.board_slug,
        "name": state.board_name,
        "aliceState": alice_state,
        "volume": vol_float,
        "volumeLevel": vol_level,
        "micsMuted": muted,
        "playing": playing,
        "lastSpokenText": last_spoken,
        "playerState": player_state,
        "state": {
            "aliceState": alice_state,
            "volume": vol_float,
            "volumeLevel": vol_level,
            "micsMuted": muted,
            "playing": playing,
            "playerState": player_state,
        },
        "connections": peers,
    })
}

fn parse_glagol_volume_level(cmd_obj: &Value) -> Option<u32> {
    if let Some(vl) = cmd_obj.get("volumeLevel").and_then(|v| v.as_u64()) {
        return Some((vl as u32).min(10));
    }
    let vol_val = cmd_obj.get("volume")?;
    if let Some(u) = vol_val.as_u64() {
        if u <= 10 {
            return Some(u as u32);
        }
        return Some(((u as u32 + 5) / 10).min(10));
    }
    if let Some(i) = vol_val.as_i64() {
        if i <= 0 {
            return Some(0);
        }
        let u = i as u32;
        if u <= 10 {
            return Some(u);
        }
        return Some(((u + 5) / 10).min(10));
    }
    if let Some(f) = vol_val.as_f64() {
        let clamped = f.clamp(0.0, 100.0);
        if clamped <= 1.0 {
            return Some((clamped * 10.0).round() as u32);
        }
        if clamped <= 10.0 {
            return Some(clamped.round() as u32);
        }
        return Some((clamped / 10.0).round() as u32);
    }
    None
}

fn apply_alsa_set_volume_script(level: u32) {
    let script = Path::new("/system/vendor/quasar/set_volume.sh");
    if script.exists() {
        let pct = (level.min(10) * 10).to_string();
        let _ = Command::new(script).arg(&pct).output();
    }
}

fn extract_glagol_text_phrase(cmd_obj: &Value) -> Option<String> {
    for key in ["text", "phrase", "utterance", "query"] {
        if let Some(s) = cmd_obj.get(key).and_then(|v| v.as_str()) {
            let trimmed = s.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    if let Some(inner) = cmd_obj.get("payload") {
        for key in ["text", "phrase", "utterance", "query"] {
            if let Some(s) = inner.get(key).and_then(|v| v.as_str()) {
                let trimmed = s.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
    }
    if let Some(ev) = cmd_obj.get("event").and_then(|e| e.get("payload")) {
        if let Some(s) = ev.get("text").and_then(|v| v.as_str()) {
            let trimmed = s.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

/// Executes a Glagol JSON command (`play`, `pause`, `stop`, `prev`, `next`, `setVolume`,
/// `sendText` / `vins`) against `BridgeState` and returns the updated speaker state JSON.
pub fn execute_glagol_command_json(
    state: &Arc<BridgeState>,
    raw_json: &str,
    peer_addr: &str,
    protocol: &str,
) -> Result<Value, String> {
    let parsed: Value =
        serde_json::from_str(raw_json).map_err(|e| format!("invalid Glagol JSON: {}", e))?;

    // Support both flat `{"command": "..."}` and official Glagol envelope `{"id": "...", "payload": {"command": "..."}}`
    let cmd_obj = if parsed.get("command").is_some() {
        &parsed
    } else if let Some(inner) = parsed.get("payload").filter(|p| p.is_object()) {
        inner
    } else {
        &parsed
    };

    let peer_id = parsed
        .get("peerId")
        .or_else(|| parsed.get("deviceId"))
        .or_else(|| cmd_obj.get("peerId"))
        .and_then(|v| v.as_str())
        .unwrap_or(peer_addr);

    let cmd_name = cmd_obj
        .get("command")
        .and_then(|v| v.as_str())
        .unwrap_or("getState");

    upsert_glagol_peer(
        state,
        peer_id,
        peer_addr,
        protocol,
        Some(cmd_name.to_string()),
        true,
    );

    match cmd_name {
        "play" => {
            state.handle_playback_control(0);
            state.log(format!("Glagol :1961 ({}) -> command=play", peer_addr));
        }
        "pause" => {
            state.handle_playback_control(1);
            state.log(format!("Glagol :1961 ({}) -> command=pause", peer_addr));
        }
        "stop" => {
            state.player_playing.store(false, Ordering::SeqCst);
            let cur_seq = state.dialog_seq.load(Ordering::SeqCst);
            state.hijacked_seq.store(cur_seq, Ordering::SeqCst);
            state.set_alice_state(AliceState::Idle);
            state.append_maind_log("[sdk] PlaybackControlCapability: STOP (playing=false)");
            state.log(format!("Glagol :1961 ({}) -> command=stop", peer_addr));
        }
        "next" => {
            state.handle_playback_control(6);
            state.log(format!("Glagol :1961 ({}) -> command=next", peer_addr));
        }
        "prev" | "previous" => {
            state.handle_playback_control(7);
            state.log(format!("Glagol :1961 ({}) -> command=prev", peer_addr));
        }
        "setVolume" | "volume" => {
            let level = parse_glagol_volume_level(cmd_obj)
                .or_else(|| parse_glagol_volume_level(&parsed))
                .ok_or_else(|| "setVolume missing numeric 'volume' or 'volumeLevel'".to_string())?;
            set_host_volume(state, level);
            apply_alsa_set_volume_script(level);
            state.log(format!(
                "Glagol :1961 ({}) -> command=setVolume (level={}/10)",
                peer_addr, level
            ));
        }
        "sendText" | "vins" | "text" | "serverAction" | "directive" => {
            let phrase = extract_glagol_text_phrase(cmd_obj)
                .or_else(|| extract_glagol_text_phrase(&parsed))
                .ok_or_else(|| format!("{} missing non-empty 'text' field", cmd_name))?;
            state.log(format!(
                "Glagol :1961 ({}) -> command={} text=\"{}\"",
                peer_addr, cmd_name, phrase
            ));
            let _ = handle_inject_voice(state, phrase, None, None, false);
        }
        "ping" | "getState" | "state" => {
            state.log(format!("Glagol :1961 ({}) -> command={}", peer_addr, cmd_name));
        }
        other => {
            state.log(format!(
                "Glagol :1961 ({}) -> unrecognized command='{}' (returning state)",
                peer_addr, other
            ));
        }
    }

    let mut out = build_glagol_state_json(state);
    if let Some(req_id) = parsed.get("id").and_then(|v| v.as_str()) {
        if let Some(obj) = out.as_object_mut() {
            obj.insert("requestId".to_string(), Value::String(req_id.to_string()));
        }
    }
    Ok(out)
}

/// Pure-Rust SHA-1 digest (RFC 3174) used solely for RFC 6455 `Sec-WebSocket-Accept` computation.
fn sha1_digest(data: &[u8]) -> [u8; 20] {
    let mut h0: u32 = 0x6745_2301;
    let mut h1: u32 = 0xEFCD_AB89;
    let mut h2: u32 = 0x98BA_DCFE;
    let mut h3: u32 = 0x1032_5476;
    let mut h4: u32 = 0xC3D2_E1F0;

    let bit_len = (data.len() as u64) * 8;
    let mut padded = Vec::with_capacity(data.len() + 72);
    padded.extend_from_slice(data);
    padded.push(0x80);
    while (padded.len() % 64) != 56 {
        padded.push(0x00);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in padded.chunks_exact(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            let off = i * 4;
            w[i] = u32::from_be_bytes([chunk[off], chunk[off + 1], chunk[off + 2], chunk[off + 3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }

        let mut a = h0;
        let mut b = h1;
        let mut c = h2;
        let mut d = h3;
        let mut e = h4;

        for (i, &wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A82_7999u32),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1u32),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDCu32),
                _ => (b ^ c ^ d, 0xCA62_C1D6u32),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }

        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
        h4 = h4.wrapping_add(e);
    }

    let mut out = [0u8; 20];
    out[0..4].copy_from_slice(&h0.to_be_bytes());
    out[4..8].copy_from_slice(&h1.to_be_bytes());
    out[8..12].copy_from_slice(&h2.to_be_bytes());
    out[12..16].copy_from_slice(&h3.to_be_bytes());
    out[16..20].copy_from_slice(&h4.to_be_bytes());
    out
}

fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;

        out.push(TABLE[((triple >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(triple & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// Computes the RFC 6455 `Sec-WebSocket-Accept` header value for a client `Sec-WebSocket-Key`.
pub fn websocket_accept_key(sec_websocket_key: &str) -> String {
    let mut input = Vec::with_capacity(sec_websocket_key.trim().len() + WS_GUID.len());
    input.extend_from_slice(sec_websocket_key.trim().as_bytes());
    input.extend_from_slice(WS_GUID.as_bytes());
    let digest = sha1_digest(&input);
    base64_encode(&digest)
}

/// Encodes an unmasked server-to-client RFC 6455 WebSocket frame (`opcode = 0x1` for text).
pub fn encode_ws_frame(opcode: u8, payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(payload.len() + 10);
    frame.push(0x80 | (opcode & 0x0F));
    if payload.len() < 126 {
        frame.push(payload.len() as u8);
    } else if payload.len() <= 65535 {
        frame.push(126);
        frame.extend_from_slice(&(payload.len() as u16).to_be_bytes());
    } else {
        frame.push(127);
        frame.extend_from_slice(&(payload.len() as u64).to_be_bytes());
    }
    frame.extend_from_slice(payload);
    frame
}

/// Reads a single RFC 6455 WebSocket frame from `reader`, unmasking client payload bytes if masked.
pub fn read_ws_frame<R: Read>(reader: &mut R) -> std::io::Result<(u8, Vec<u8>)> {
    let mut hdr = [0u8; 2];
    reader.read_exact(&mut hdr)?;
    let opcode = hdr[0] & 0x0F;
    let masked = (hdr[1] & 0x80) != 0;
    let len_code = (hdr[1] & 0x7F) as usize;

    let payload_len = if len_code < 126 {
        len_code
    } else if len_code == 126 {
        let mut ext = [0u8; 2];
        reader.read_exact(&mut ext)?;
        u16::from_be_bytes(ext) as usize
    } else {
        let mut ext = [0u8; 8];
        reader.read_exact(&mut ext)?;
        let len64 = u64::from_be_bytes(ext);
        if len64 > 1024 * 1024 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "WebSocket frame too large",
            ));
        }
        len64 as usize
    };

    let mut mask_key = [0u8; 4];
    if masked {
        reader.read_exact(&mut mask_key)?;
    }

    let mut payload = vec![0u8; payload_len];
    reader.read_exact(&mut payload)?;
    if masked {
        for (i, b) in payload.iter_mut().enumerate() {
            *b ^= mask_key[i & 3];
        }
    }
    Ok((opcode, payload))
}

fn write_http_json_response(stream: &mut TcpStream, status_line: &str, body_val: &Value) {
    let body = serde_json::to_string(body_val).unwrap_or_else(|_| "{}".to_string());
    let resp = format!(
        "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        status_line,
        body.len(),
        body
    );
    let _ = stream.write_all(resp.as_bytes());
    let _ = stream.flush();
}

/// Handles a single external Glagol HTTP or WebSocket client connection on `:1961`.
pub fn handle_glagol_external_client(mut stream: TcpStream, state: Arc<BridgeState>) {
    let peer_addr = stream
        .peer_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| "127.0.0.1:0".to_string());
    let _ = stream.set_nodelay(true);
    let _ = stream.set_read_timeout(Some(Duration::from_millis(2000)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(1000)));

    let mut buf = Vec::with_capacity(2048);
    let mut temp = [0u8; 1024];
    let mut header_end = None;
    while buf.len() < 32768 {
        match stream.read(&mut temp) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&temp[..n]);
                if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                    header_end = Some(pos + 4);
                    break;
                }
            }
            Err(_) => break,
        }
    }

    let Some(hdr_end) = header_end else {
        return;
    };

    let header_str = String::from_utf8_lossy(&buf[..hdr_end]).into_owned();
    let mut lines = header_str.lines();
    let request_line = lines.next().unwrap_or("");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("GET");
    let path = parts.next().unwrap_or("/");

    let mut content_length = 0usize;
    let mut ws_key: Option<String> = None;
    let mut is_ws_upgrade = false;

    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            let k_lower = k.trim().to_ascii_lowercase();
            let v_trim = v.trim();
            if k_lower == "content-length" {
                content_length = v_trim.parse::<usize>().unwrap_or(0);
            } else if k_lower == "sec-websocket-key" {
                ws_key = Some(v_trim.to_string());
            } else if k_lower == "upgrade" && v_trim.eq_ignore_ascii_case("websocket") {
                is_ws_upgrade = true;
            }
        }
    }

    if is_ws_upgrade {
        if let Some(key) = ws_key {
            let accept = websocket_accept_key(&key);
            let handshake = format!(
                "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n",
                accept
            );
            if stream.write_all(handshake.as_bytes()).is_err() {
                return;
            }
            let _ = stream.flush();

            let ws_peer_id = format!("ws-{}", peer_addr);
            upsert_glagol_peer(&state, &ws_peer_id, &peer_addr, "websocket", None, true);
            state.log(format!(
                "Glagol :1961 WebSocket connected from {}",
                peer_addr
            ));

            // Immediately push initial speaker state frame upon WebSocket connection
            let initial_state = serde_json::to_string(&build_glagol_state_json(&state))
                .unwrap_or_else(|_| "{}".to_string());
            let init_frame = encode_ws_frame(0x1, initial_state.as_bytes());
            if stream.write_all(&init_frame).is_err() {
                return;
            }
            let _ = stream.flush();

            let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
            while let Ok((opcode, payload)) = read_ws_frame(&mut stream) {
                match opcode {
                    0x1 => {
                        let text = String::from_utf8_lossy(&payload);
                        let resp_val =
                            match execute_glagol_command_json(&state, &text, &peer_addr, "websocket")
                            {
                                Ok(v) => v,
                                Err(err) => json!({
                                    "status": "error",
                                    "error": err,
                                    "state": build_glagol_state_json(&state),
                                }),
                            };
                        let resp_str =
                            serde_json::to_string(&resp_val).unwrap_or_else(|_| "{}".to_string());
                        let frame = encode_ws_frame(0x1, resp_str.as_bytes());
                        if stream.write_all(&frame).is_err() {
                            break;
                        }
                        let _ = stream.flush();
                    }
                    0x9 => {
                        let pong = encode_ws_frame(0xA, &payload);
                        if stream.write_all(&pong).is_err() {
                            break;
                        }
                        let _ = stream.flush();
                    }
                    0x8 => {
                        let close = encode_ws_frame(0x8, &[]);
                        let _ = stream.write_all(&close);
                        let _ = stream.flush();
                        break;
                    }
                    _ => {}
                }
            }
            return;
        }
    }

    // Read remaining HTTP request body if Content-Length was specified
    while buf.len() < hdr_end + content_length && content_length <= 65536 {
        match stream.read(&mut temp) {
            Ok(0) => break,
            Ok(n) => buf.extend_from_slice(&temp[..n]),
            Err(_) => break,
        }
    }
    let body_slice = if buf.len() >= hdr_end {
        let end = (hdr_end + content_length).min(buf.len());
        &buf[hdr_end..end]
    } else {
        &[]
    };
    let body_str = String::from_utf8_lossy(body_slice).trim().to_string();

    if method.eq_ignore_ascii_case("POST") || (!body_str.is_empty() && path.starts_with("/command"))
    {
        match execute_glagol_command_json(&state, &body_str, &peer_addr, "http") {
            Ok(resp_val) => write_http_json_response(&mut stream, "200 OK", &resp_val),
            Err(err) => {
                let err_val = json!({
                    "status": "error",
                    "error": err,
                    "state": build_glagol_state_json(&state),
                });
                write_http_json_response(&mut stream, "400 Bad Request", &err_val);
            }
        }
    } else {
        let http_peer_id = format!("http-{}", peer_addr);
        upsert_glagol_peer(
            &state,
            &http_peer_id,
            &peer_addr,
            "http",
            Some(format!("{} {}", method, path)),
            true,
        );
        let snap = build_glagol_state_json(&state);
        write_http_json_response(&mut stream, "200 OK", &snap);
    }
}

/// Spawns the external Glagol HTTP / WebSocket server on `0.0.0.0:1961` in `Stub Mode`.
pub fn spawn_glagol_external_server(state: Arc<BridgeState>) {
    thread::spawn(move || {
        let Ok(listener) = TcpListener::bind("0.0.0.0:1961") else {
            state.log("Port 0.0.0.0:1961 already bound by native maind");
            return;
        };
        state.log("Listening on 0.0.0.0:1961 (external Glagol HTTP/WebSocket stateful API)");
        for stream in listener.incoming().flatten() {
            let st = state.clone();
            thread::spawn(move || {
                handle_glagol_external_client(stream, st);
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::{LogMessage, QuasarMessage, QuasarMessageType};
    use crate::state::BoardProfile;

    fn decode_hex(hex: &str) -> Vec<u8> {
        let clean: Vec<u8> = hex.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
        assert_eq!(clean.len() % 2, 0, "hex string must have even length");
        clean
            .chunks(2)
            .map(|pair| {
                let s = std::str::from_utf8(pair).unwrap();
                u8::from_str_radix(s, 16).unwrap()
            })
            .collect()
    }

    fn make_test_state() -> Arc<BridgeState> {
        let profile = BoardProfile {
            board_slug: "yandexmini2".to_string(),
            board_name: "Yandex Station Mini 2".to_string(),
            sysfs_prefix: "mini2".to_string(),
            ring_leds: 4,
            logo_leds: 0,
            ambilight_leds: 0,
            has_clock: true,
        };
        Arc::new(BridgeState::new(profile, None, None, None))
    }

    fn split_framed_packets(mut buf: &[u8]) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        while !buf.is_empty() {
            assert!(buf.len() >= 4, "truncated 4-byte length prefix");
            let len = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
            buf = &buf[4..];
            assert!(buf.len() >= len, "truncated frame payload");
            out.push(buf[..len].to_vec());
            buf = &buf[len..];
        }
        out
    }

    #[test]
    fn test_glagold_ipc_9899_fixtures_legacy_and_envelope() {
        let state = make_test_state();

        // Exact framed binary captures from `q glagol-connections` (`maind --glagol-connections`):
        // 1. Legacy direct field 185: 05000000ca0b024200
        // 2. Modern LITE_RUNTIME envelope 252/253 (type = 185): 09000000e20f024200e80fb901
        let cases = [
            ("05000000ca0b024200", QuasarWireFormat::LegacyDirect),
            ("09000000e20f024200e80fb901", QuasarWireFormat::Envelope252),
        ];

        for (framed_hex, expected_wf) in cases {
            let framed = decode_hex(framed_hex);
            let declared_len =
                u32::from_le_bytes([framed[0], framed[1], framed[2], framed[3]]) as usize;
            assert_eq!(declared_len + 4, framed.len());

            let (reply_bundle, wf) =
                handle_glagold_ipc_payload(&state, &framed[4..]).expect("valid glagold request");
            assert_eq!(wf, expected_wf);

            let packets = split_framed_packets(&reply_bundle);
            assert_eq!(
                packets.len(),
                3,
                "expected [GlagolDiscoveryResult(187), GlagoldState(82), LogMessage(184)]"
            );

            // Frame 0: GlagolDiscoveryResult (type = 187)
            let qmsg_disc = QuasarMessage::decode(&packets[0]).unwrap();
            assert_eq!(qmsg_disc.wire_format, expected_wf);
            let disc_bytes = qmsg_disc
                .find_payload(QuasarMessageType::GLAGOL_DISCOVERY_RESULT)
                .expect("GLAGOL_DISCOVERY_RESULT (187)");
            let disc = GlagolDiscoveryResult::decode(disc_bytes).unwrap();
            assert!(disc.is_full_result);
            assert!(!disc.items.is_empty());
            assert_eq!(disc.items[0].device_id.as_deref(), Some("EMU0001"));
            assert_eq!(disc.items[0].platform.as_deref(), Some("yandexmini2"));

            // Frame 1: GlagoldState (type = 82)
            let qmsg_state = QuasarMessage::decode(&packets[1]).unwrap();
            assert_eq!(qmsg_state.wire_format, expected_wf);
            let state_bytes = qmsg_state
                .find_payload(QuasarMessageType::GLAGOLD_STATE)
                .expect("GLAGOLD_STATE (82)");
            let gstate = GlagoldState::decode(state_bytes).unwrap();
            assert!(gstate.cluster_device_list.contains(&"EMU0001".to_string()));
            assert_eq!(gstate.public_ip.as_deref(), Some("10.0.2.15"));
            assert!(!gstate.connections.is_empty());

            // Frame 2: LogMessage (type = 184) required by `maind --glagol-connections`
            let qmsg_log = QuasarMessage::decode(&packets[2]).unwrap();
            assert_eq!(qmsg_log.wire_format, expected_wf);
            let log_bytes = qmsg_log
                .find_payload(QuasarMessageType::LOG_MESSAGE)
                .expect("LOG_MESSAGE (184)");
            let log_msg = LogMessage::decode(log_bytes).unwrap();
            let json_str = log_msg.msg.expect("LogMessage.msg JSON string");
            let parsed_json: Value =
                serde_json::from_str(&json_str).expect("LogMessage.msg must be valid JSON");
            assert_eq!(parsed_json["device_id"], "EMU0001");
            assert_eq!(parsed_json["platform"], "yandexmini2");
            assert!(parsed_json["connections"].is_array());
        }
        assert_eq!(state.ipc_error_count.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_glagol_1961_commands_and_9899_peer_reflection() {
        let state = make_test_state();

        // 1. Play command
        let resp =
            execute_glagol_command_json(&state, r#"{"command":"play"}"#, "10.0.2.2:54321", "http")
                .unwrap();
        assert_eq!(resp["id"], "EMU0001");
        assert_eq!(resp["playing"], true);
        assert_eq!(resp["playerState"]["hasPause"], true);
        assert_eq!(resp["playerState"]["hasPlay"], false);

        // 2. Next & Prev track commands
        let resp_next = execute_glagol_command_json(
            &state,
            r#"{"id":"req-2","payload":{"command":"next"}}"#,
            "10.0.2.2:54321",
            "websocket",
        )
        .unwrap();
        assert_eq!(resp_next["requestId"], "req-2");
        assert_eq!(resp_next["playerState"]["title"], "Track #2");

        let resp_prev = execute_glagol_command_json(
            &state,
            r#"{"command":"prev"}"#,
            "10.0.2.2:54321",
            "websocket",
        )
        .unwrap();
        assert_eq!(resp_prev["playerState"]["title"], "Track #1");

        // 3. Pause & Stop commands
        let resp_pause = execute_glagol_command_json(
            &state,
            r#"{"command":"pause"}"#,
            "10.0.2.2:54321",
            "websocket",
        )
        .unwrap();
        assert_eq!(resp_pause["playing"], false);

        state.player_playing.store(true, Ordering::SeqCst);
        let resp_stop = execute_glagol_command_json(
            &state,
            r#"{"command":"stop"}"#,
            "10.0.2.2:54321",
            "websocket",
        )
        .unwrap();
        assert_eq!(resp_stop["playing"], false);
        assert_eq!(resp_stop["aliceState"], "IDLE");

        // 4. setVolume command (both float 0.0..1.0 and integer step 0..10)
        let resp_vol = execute_glagol_command_json(
            &state,
            r#"{"command":"setVolume","volume":0.4}"#,
            "10.0.2.2:54321",
            "websocket",
        )
        .unwrap();
        assert_eq!(resp_vol["volumeLevel"], 4);
        assert_eq!(state.volume_level.load(Ordering::Relaxed), 4);

        // 5. sendText / vins command -> triggers handle_inject_voice -> LISTENING state
        let resp_text = execute_glagol_command_json(
            &state,
            r#"{"command":"sendText","text":"какая погода"}"#,
            "10.0.2.2:54321",
            "websocket",
        )
        .unwrap();
        assert_eq!(resp_text["aliceState"], "LISTENING");
        assert_eq!(state.alice_state(), AliceState::Listening);

        // 6. Verify that the connected :1961 peer is reflected on :9899 discovery & LogMessage
        let env_req = decode_hex("e20f024200e80fb901");
        let (reply_bundle, _) = handle_glagold_ipc_payload(&state, &env_req).unwrap();
        let packets = split_framed_packets(&reply_bundle);

        let qmsg_disc = QuasarMessage::decode(&packets[0]).unwrap();
        let disc = GlagolDiscoveryResult::decode(
            qmsg_disc
                .find_payload(QuasarMessageType::GLAGOL_DISCOVERY_RESULT)
                .unwrap(),
        )
        .unwrap();
        assert!(
            disc.items
                .iter()
                .any(|item| item.address.as_deref() == Some("10.0.2.2:54321")),
            "connected :1961 peer must appear in GlagolDiscoveryResult.items on :9899"
        );

        let qmsg_log = QuasarMessage::decode(&packets[2]).unwrap();
        let log_msg = LogMessage::decode(
            qmsg_log
                .find_payload(QuasarMessageType::LOG_MESSAGE)
                .unwrap(),
        )
        .unwrap();
        let log_json: Value = serde_json::from_str(&log_msg.msg.unwrap()).unwrap();
        assert!(
            log_json["connections"]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["address"] == "10.0.2.2:54321"),
            "connected :1961 peer must appear in q glagol-connections JSON on :9899"
        );
    }

    #[test]
    fn test_websocket_rfc6455_handshake_and_http_round_trip() {
        // RFC 6455 Section 4.2.2 canonical test vector
        assert_eq!(
            websocket_accept_key("dGhlIHNhbXBsZSBub25jZQ=="),
            "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        );

        let state = make_test_state();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let st_srv = state.clone();
        let srv_handle = thread::spawn(move || {
            for _ in 0..2 {
                if let Ok((stream, _)) = listener.accept() {
                    handle_glagol_external_client(stream, st_srv.clone());
                }
            }
        });

        // 1. HTTP POST /command
        {
            let mut client = TcpStream::connect(addr).unwrap();
            let body = r#"{"command":"setVolume","volume":0.8}"#;
            let req = format!(
                "POST /command HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                addr,
                body.len(),
                body
            );
            client.write_all(req.as_bytes()).unwrap();
            let mut resp = String::new();
            client.read_to_string(&mut resp).unwrap();
            assert!(resp.starts_with("HTTP/1.1 200 OK"));
            let json_body = resp.split("\r\n\r\n").nth(1).unwrap();
            let val: Value = serde_json::from_str(json_body).unwrap();
            assert_eq!(val["volumeLevel"], 8);
        }

        // 2. WebSocket upgrade + masked client text frame
        {
            let mut ws_client = TcpStream::connect(addr).unwrap();
            let upgrade_req = format!(
                "GET / HTTP/1.1\r\nHost: {}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n",
                addr
            );
            ws_client.write_all(upgrade_req.as_bytes()).unwrap();

            let mut hdr_buf = Vec::new();
            let mut b = [0u8; 1];
            while !hdr_buf.ends_with(b"\r\n\r\n") {
                ws_client.read_exact(&mut b).unwrap();
                hdr_buf.push(b[0]);
            }
            let hdr_str = String::from_utf8_lossy(&hdr_buf);
            assert!(hdr_str.contains("101 Switching Protocols"));
            assert!(hdr_str.contains("s3pPLMBiTxaQ9kYGzzhZRbK+xOo="));

            // Read initial state frame pushed on connect
            let (op_init, init_payload) = read_ws_frame(&mut ws_client).unwrap();
            assert_eq!(op_init, 0x1);
            let init_val: Value = serde_json::from_slice(&init_payload).unwrap();
            assert_eq!(init_val["id"], "EMU0001");
            assert_eq!(init_val["volumeLevel"], 8);

            // Send masked text frame: `{"command":"play"}`
            let cmd_bytes = br#"{"command":"play"}"#;
            let mask = [0x12u8, 0x34, 0x56, 0x78];
            let mut masked_frame = vec![0x81, 0x80 | (cmd_bytes.len() as u8)];
            masked_frame.extend_from_slice(&mask);
            for (i, ch) in cmd_bytes.iter().enumerate() {
                masked_frame.push(ch ^ mask[i & 3]);
            }
            ws_client.write_all(&masked_frame).unwrap();

            let (op_resp, resp_payload) = read_ws_frame(&mut ws_client).unwrap();
            assert_eq!(op_resp, 0x1);
            let resp_val: Value = serde_json::from_slice(&resp_payload).unwrap();
            assert_eq!(resp_val["playing"], true);

            // Send close frame
            ws_client.write_all(&[0x88, 0x00]).unwrap();
        }

        let _ = srv_handle.join();
    }
}
