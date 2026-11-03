//! Pure-Rust D-Bus System Bus & `org.bluez` / `org.bluealsa` Wire-Protocol Responder (`Full Mode`).
//!
//! ## Why `maind --platformd` Requires Three-Stage D-Bus / BlueZ Initialization in `Full Mode`
//!
//! On BlueZ-based Quasar firmwares (`0.13+`, `0.369+`, `0.373+`: `bergamot`, `yandexmini2`,
//! `yandexmini2nc`, `plum`, `plumnc`, `mango`, `station2`, `yandexmicro`, `cucumber`, `orion`),
//! launching the native monolithic daemon (`maind --platformd` via `/system/vendor/quasar/onboot.sh`)
//! in `Full Mode` (`quasar.mode=full`) initializes Bluetooth and BLE onboarding via `sdbus-c++`
//! (`libsystemd` `sd-bus`) before bringing up the main event loop and logger. Without three
//! coordinated environment invariants, `maind --platformd` terminates with an unhandled
//! `sdbus::Error` exception:
//!
//! 1. **Stage 1 — Unified `/run` and `/var/run` (`/var/run -> /run` symlink)**:
//!    `dbus-daemon --system` and `sdbus-c++` reference `/run/dbus/system_bus_socket` and
//!    `/var/run/dbus/system_bus_socket` interchangeably across Buildroot generations. Mounting
//!    two independent `tmpfs` instances on `/run` and `/var/run` hides the system bus socket
//!    from `sdbus-c++`, aborting `maind --platformd` with:
//!    `sdbus::Error: [org.freedesktop.DBus.Error.FileNotFound] Failed to open bus (No such file or directory)`.
//!
//! 2. **Stage 2 — Setuid helper permissions (`04750` `root:dbus` / `0:85`) on `/usr/libexec/dbus-daemon-launch-helper`**:
//!    When `dbus-daemon --system` attempts on-demand service activation, it verifies that
//!    `/usr/libexec/dbus-daemon-launch-helper` is owned by `0:85` (`root:dbus`) with mode `04750`
//!    (`S_ISUID | 0750`). Firmware rootfs dumps with `0755` `0:0` cause `dbus-daemon` to reject
//!    activation and abort `maind --platformd` with:
//!    `sdbus::Error: [org.freedesktop.DBus.Error.Spawn.PermissionsInvalid] The permission of the setuid helper is not correct`.
//!
//! 3. **Stage 3 — Pre-started `dbus-daemon --system` and `org.bluez` + `org.bluealsa` D-Bus responder**:
//!    Even when `dbus-daemon-launch-helper` has `04750` `0:85` permissions, `dbus-daemon` spawns
//!    `/usr/libexec/bluetooth/bluetoothd -d` (from `/usr/share/dbus-1/system-services/org.bluez.service`),
//!    which exits immediately in QEMU due to the absence of a kernel Bluetooth HCI controller,
//!    aborting `maind --platformd` with:
//!    `sdbus::Error: [org.freedesktop.DBus.Error.NoReply] Message recipient disconnected from message bus without replying`.
//!    Starting `dbus-daemon --system` and registering this pure-Rust D-Bus wire-protocol responder
//!    for `"org.bluez"` and `"org.bluealsa"` on `/run/dbus/system_bus_socket` (with a standalone
//!    Unix socket bus server fallback when `dbus-daemon` is absent or faults, e.g. on `orion`)
//!    prior to `/system/vendor/quasar/onboot.sh` satisfies all synchronous `sdbus-c++` startup calls
//!    (`GetManagedObjects`, `GetPCMs`, `GetAll`, `Get`, `Introspect`, `RegisterAgent`, etc.).

use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

pub const SYSTEM_BUS_SOCKET_PATH: &str = "/run/dbus/system_bus_socket";
const SERVER_GUID_HEX: &str = "0123456789abcdef0123456789abcdef";

pub const DBUS_ENDIAN_LE: u8 = b'l';
pub const DBUS_ENDIAN_BE: u8 = b'B';
pub const DBUS_PROTO_VERSION: u8 = 1;

pub const DBUS_MSG_METHOD_CALL: u8 = 1;
pub const DBUS_MSG_METHOD_RETURN: u8 = 2;
pub const DBUS_MSG_SIGNAL: u8 = 4;

pub const DBUS_FLAG_NO_REPLY_EXPECTED: u8 = 0x01;

pub const FIELD_PATH: u8 = 1;
pub const FIELD_INTERFACE: u8 = 2;
pub const FIELD_MEMBER: u8 = 3;
pub const FIELD_ERROR_NAME: u8 = 4;
pub const FIELD_REPLY_SERIAL: u8 = 5;
pub const FIELD_DESTINATION: u8 = 6;
pub const FIELD_SENDER: u8 = 7;
pub const FIELD_SIGNATURE: u8 = 8;

///DBUS_NAME_FLAG_ALLOW_REPLACEMENT (1) | DBUS_NAME_FLAG_REPLACE_EXISTING (2) | DBUS_NAME_FLAG_DO_NOT_QUEUE (4)
pub const DBUS_NAME_FLAGS_CLAIM: u32 = 0x07;
pub const DBUS_REQUEST_NAME_REPLY_PRIMARY_OWNER: u32 = 1;
pub const DBUS_START_REPLY_ALREADY_RUNNING: u32 = 2;

static DBUS_STUB_STARTED: AtomicBool = AtomicBool::new(false);

/// Parsed representation of a D-Bus wire-protocol message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbusMessage {
    pub endian: u8,
    pub msg_type: u8,
    pub flags: u8,
    pub version: u8,
    pub serial: u32,
    pub path: Option<String>,
    pub interface: Option<String>,
    pub member: Option<String>,
    pub error_name: Option<String>,
    pub reply_serial: Option<u32>,
    pub destination: Option<String>,
    pub sender: Option<String>,
    pub signature: Option<String>,
    pub body: Vec<u8>,
}

fn read_u32_endian(buf: &[u8], pos: usize, endian: u8) -> Option<u32> {
    let bytes: [u8; 4] = buf.get(pos..pos + 4)?.try_into().ok()?;
    Some(if endian == DBUS_ENDIAN_BE {
        u32::from_be_bytes(bytes)
    } else {
        u32::from_le_bytes(bytes)
    })
}

fn align_up(pos: usize, align: usize) -> usize {
    (pos + align - 1) & !(align - 1)
}

fn pad_vec_to(buf: &mut Vec<u8>, align: usize) {
    while buf.len() % align != 0 {
        buf.push(0);
    }
}

/// Computes the total frame length in bytes (`header_padded + body_len`) from a 16-byte D-Bus fixed header.
pub fn frame_len_from_fixed_header(fixed_hdr: &[u8; 16]) -> Result<usize, String> {
    let endian = fixed_hdr[0];
    if endian != DBUS_ENDIAN_LE && endian != DBUS_ENDIAN_BE {
        return Err(format!("invalid D-Bus endian byte: 0x{:02x}", endian));
    }
    let version = fixed_hdr[3];
    if version != DBUS_PROTO_VERSION {
        return Err(format!("unsupported D-Bus protocol version: {}", version));
    }
    let body_len = read_u32_endian(fixed_hdr, 4, endian).unwrap_or(0) as usize;
    let fields_len = read_u32_endian(fixed_hdr, 12, endian).unwrap_or(0) as usize;
    if body_len > 16 * 1024 * 1024 || fields_len > 4 * 1024 * 1024 {
        return Err("D-Bus message exceeds sanity size limit".to_string());
    }
    let header_padded = align_up(16 + fields_len, 8);
    Ok(header_padded + body_len)
}

/// Parses a complete D-Bus message frame (`fixed_header + header_fields + padding + body`).
pub fn parse_dbus_message(frame: &[u8]) -> Result<DbusMessage, String> {
    if frame.len() < 16 {
        return Err("D-Bus frame shorter than 16-byte fixed header".to_string());
    }
    let fixed: [u8; 16] = frame[0..16].try_into().unwrap();
    let expected_len = frame_len_from_fixed_header(&fixed)?;
    if frame.len() < expected_len {
        return Err(format!(
            "truncated D-Bus frame: got {} bytes, expected {}",
            frame.len(),
            expected_len
        ));
    }

    let endian = fixed[0];
    let msg_type = fixed[1];
    let flags = fixed[2];
    let version = fixed[3];
    let body_len = read_u32_endian(&fixed, 4, endian).unwrap() as usize;
    let serial = read_u32_endian(&fixed, 8, endian).unwrap();
    let fields_len = read_u32_endian(&fixed, 12, endian).unwrap() as usize;

    let mut path = None;
    let mut interface = None;
    let mut member = None;
    let mut error_name = None;
    let mut reply_serial = None;
    let mut destination = None;
    let mut sender = None;
    let mut signature = None;

    let fields_end = 16 + fields_len;
    let mut pos = 16usize;
    while pos < fields_end {
        pos = align_up(pos, 8);
        if pos >= fields_end {
            break;
        }
        let field_code = frame[pos];
        pos += 1;
        if pos >= fields_end {
            break;
        }
        let sig_len = frame[pos] as usize;
        pos += 1;
        if pos + sig_len + 1 > fields_end {
            break;
        }
        let sig = &frame[pos..pos + sig_len];
        pos += sig_len + 1; // skip NUL terminator of signature

        match sig {
            b"s" | b"o" => {
                pos = align_up(pos, 4);
                let Some(s_len) = read_u32_endian(frame, pos, endian) else {
                    break;
                };
                pos += 4;
                let s_len = s_len as usize;
                if pos + s_len + 1 > fields_end {
                    break;
                }
                let val = String::from_utf8_lossy(&frame[pos..pos + s_len]).into_owned();
                pos += s_len + 1;
                match field_code {
                    FIELD_PATH => path = Some(val),
                    FIELD_INTERFACE => interface = Some(val),
                    FIELD_MEMBER => member = Some(val),
                    FIELD_ERROR_NAME => error_name = Some(val),
                    FIELD_DESTINATION => destination = Some(val),
                    FIELD_SENDER => sender = Some(val),
                    _ => {}
                }
            }
            b"g" => {
                if pos >= fields_end {
                    break;
                }
                let g_len = frame[pos] as usize;
                pos += 1;
                if pos + g_len + 1 > fields_end {
                    break;
                }
                let val = String::from_utf8_lossy(&frame[pos..pos + g_len]).into_owned();
                pos += g_len + 1;
                if field_code == FIELD_SIGNATURE {
                    signature = Some(val);
                }
            }
            b"u" | b"i" | b"b" | b"h" => {
                pos = align_up(pos, 4);
                let Some(val) = read_u32_endian(frame, pos, endian) else {
                    break;
                };
                pos += 4;
                if field_code == FIELD_REPLY_SERIAL {
                    reply_serial = Some(val);
                }
            }
            b"y" => {
                pos += 1;
            }
            b"n" | b"q" => {
                pos = align_up(pos, 2) + 2;
            }
            b"x" | b"t" | b"d" => {
                pos = align_up(pos, 8) + 8;
            }
            _ => {
                // Unknown complex header field variant; body offset is still known from `fields_len`.
                break;
            }
        }
    }

    let body_start = align_up(fields_end, 8);
    let body = frame[body_start..body_start + body_len].to_vec();

    Ok(DbusMessage {
        endian,
        msg_type,
        flags,
        version,
        serial,
        path,
        interface,
        member,
        error_name,
        reply_serial,
        destination,
        sender,
        signature,
        body,
    })
}

/// Reads a single complete D-Bus message frame from a stream.
pub fn read_dbus_message<R: Read>(reader: &mut R) -> io::Result<DbusMessage> {
    let mut fixed = [0u8; 16];
    reader.read_exact(&mut fixed)?;
    let total_len = frame_len_from_fixed_header(&fixed)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let mut frame = Vec::with_capacity(total_len);
    frame.extend_from_slice(&fixed);
    if total_len > 16 {
        frame.resize(total_len, 0);
        reader.read_exact(&mut frame[16..])?;
    }
    parse_dbus_message(&frame).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

fn push_header_field_string(fields: &mut Vec<u8>, code: u8, sig_char: u8, value: &str) {
    pad_vec_to(fields, 8);
    fields.push(code);
    fields.push(1);
    fields.push(sig_char);
    fields.push(0);
    // Offset is now `8*k + 4`, which is 4-byte aligned for the `u32` string length.
    let bytes = value.as_bytes();
    fields.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    fields.extend_from_slice(bytes);
    fields.push(0);
}

fn push_header_field_u32(fields: &mut Vec<u8>, code: u8, value: u32) {
    pad_vec_to(fields, 8);
    fields.push(code);
    fields.push(1);
    fields.push(b'u');
    fields.push(0);
    // Offset is `8*k + 4`, 4-byte aligned for `u32`.
    fields.extend_from_slice(&value.to_le_bytes());
}

fn push_header_field_signature(fields: &mut Vec<u8>, sig: &str) {
    pad_vec_to(fields, 8);
    fields.push(FIELD_SIGNATURE);
    fields.push(1);
    fields.push(b'g');
    fields.push(0);
    // `'g'` has 1-byte alignment and a 1-byte (`u8`) length prefix.
    let bytes = sig.as_bytes();
    fields.push(bytes.len() as u8);
    fields.extend_from_slice(bytes);
    fields.push(0);
}

/// Encodes a `DbusMessage` in little-endian D-Bus wire format (`version = 1`).
pub fn encode_dbus_message(msg: &DbusMessage) -> Vec<u8> {
    let mut fields = Vec::with_capacity(64);

    if let Some(ref path) = msg.path {
        push_header_field_string(&mut fields, FIELD_PATH, b'o', path);
    }
    if let Some(ref iface) = msg.interface {
        push_header_field_string(&mut fields, FIELD_INTERFACE, b's', iface);
    }
    if let Some(ref member) = msg.member {
        push_header_field_string(&mut fields, FIELD_MEMBER, b's', member);
    }
    if let Some(ref err_name) = msg.error_name {
        push_header_field_string(&mut fields, FIELD_ERROR_NAME, b's', err_name);
    }
    if let Some(reply_serial) = msg.reply_serial {
        push_header_field_u32(&mut fields, FIELD_REPLY_SERIAL, reply_serial);
    }
    if let Some(ref dst) = msg.destination {
        push_header_field_string(&mut fields, FIELD_DESTINATION, b's', dst);
    }
    if let Some(ref sender) = msg.sender {
        push_header_field_string(&mut fields, FIELD_SENDER, b's', sender);
    }
    if let Some(ref sig) = msg.signature {
        if !sig.is_empty() {
            push_header_field_signature(&mut fields, sig);
        }
    }

    let fields_len = fields.len() as u32;
    let body_len = msg.body.len() as u32;
    let serial = msg.serial.max(1);

    let mut out = Vec::with_capacity(align_up(16 + fields.len(), 8) + msg.body.len());
    out.push(DBUS_ENDIAN_LE);
    out.push(msg.msg_type);
    out.push(msg.flags);
    out.push(DBUS_PROTO_VERSION);
    out.extend_from_slice(&body_len.to_le_bytes());
    out.extend_from_slice(&serial.to_le_bytes());
    out.extend_from_slice(&fields_len.to_le_bytes());
    out.extend_from_slice(&fields);
    // Align total header to 8-byte boundary before appending message body.
    pad_vec_to(&mut out, 8);
    out.extend_from_slice(&msg.body);
    out
}

/// Encodes a single D-Bus `STRING` (`s`) or `OBJECT_PATH` (`o`) body payload.
pub fn encode_dbus_string(s: &str) -> Vec<u8> {
    let bytes = s.as_bytes();
    let mut body = Vec::with_capacity(4 + bytes.len() + 1);
    body.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    body.extend_from_slice(bytes);
    body.push(0);
    body
}

/// Encodes an array of D-Bus strings (`as`).
pub fn encode_dbus_string_array(items: &[&str]) -> Vec<u8> {
    let mut elems = Vec::new();
    for item in items {
        pad_vec_to(&mut elems, 4);
        let b = item.as_bytes();
        elems.extend_from_slice(&(b.len() as u32).to_le_bytes());
        elems.extend_from_slice(b);
        elems.push(0);
    }
    let mut body = Vec::with_capacity(4 + elems.len());
    body.extend_from_slice(&(elems.len() as u32).to_le_bytes());
    body.extend_from_slice(&elems);
    body
}

/// Encodes an empty D-Bus dictionary array (`a{oa{sa{sv}}}`, `a{oa{sv}}`, or `a{sv}`).
///
/// Per the D-Bus specification, an `ARRAY` of `DICT_ENTRY` (`{...}`) begins with a 4-byte `u32`
/// byte length (`0`) followed by alignment padding to the 8-byte boundary of the `DICT_ENTRY`
/// element type — even when the array is empty (`0` elements) — yielding an 8-byte body `[0u8; 8]`.
pub fn encode_empty_dict_array() -> Vec<u8> {
    vec![0u8; 8]
}

/// Extracts string arguments from the beginning of a D-Bus message body (e.g. `Properties.Get(iface, prop)`).
pub fn extract_leading_strings(body: &[u8], endian: u8, max_count: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    while out.len() < max_count && pos < body.len() {
        pos = align_up(pos, 4);
        let Some(s_len) = read_u32_endian(body, pos, endian) else {
            break;
        };
        pos += 4;
        let s_len = s_len as usize;
        if pos + s_len + 1 > body.len() {
            break;
        }
        out.push(String::from_utf8_lossy(&body[pos..pos + s_len]).into_owned());
        pos += s_len + 1;
    }
    out
}

/// Encodes a D-Bus `VARIANT` (`v`) response for `org.freedesktop.DBus.Properties.Get`.
pub fn encode_property_get_variant(req_body: &[u8], endian: u8) -> Vec<u8> {
    let args = extract_leading_strings(req_body, endian, 2);
    let prop_name = args.get(1).or_else(|| args.first()).map(|s| s.as_str()).unwrap_or("");

    match prop_name {
        "Powered"
        | "Discoverable"
        | "Pairable"
        | "Discovering"
        | "Connected"
        | "Paired"
        | "Trusted"
        | "Blocked"
        | "LegacyPairing"
        | "ServicesResolved" => {
            // Variant signature "b" (3 bytes: [1, b'b', 0]), 1 byte pad to 4-byte align, u32(0).
            vec![1, b'b', 0, 0, 0, 0, 0, 0]
        }
        "Class" | "DiscoverableTimeout" | "PairableTimeout" | "Version" | "Delay" => {
            // Variant signature "u" (3 bytes: [1, b'u', 0]), 1 byte pad to 4-byte align, u32(0).
            vec![1, b'u', 0, 0, 0, 0, 0, 0]
        }
        "UUIDs" | "Includes" | "SupportedIncludes" | "Codecs" => {
            // Variant signature "as" (4 bytes: [2, b'a', b's', 0]), already 4-byte aligned, u32(0) empty array.
            vec![2, b'a', b's', 0, 0, 0, 0, 0]
        }
        _ => {
            // Variant signature "s" (3 bytes: [1, b's', 0]), 1 byte pad, u32(0) len, NUL byte.
            vec![1, b's', 0, 0, 0, 0, 0, 0, 0]
        }
    }
}

/// Builds a `METHOD_CALL` to `org.freedesktop.DBus.Hello`.
pub fn build_hello_call(serial: u32) -> Vec<u8> {
    encode_dbus_message(&DbusMessage {
        endian: DBUS_ENDIAN_LE,
        msg_type: DBUS_MSG_METHOD_CALL,
        flags: 0,
        version: DBUS_PROTO_VERSION,
        serial,
        path: Some("/org/freedesktop/DBus".to_string()),
        interface: Some("org.freedesktop.DBus".to_string()),
        member: Some("Hello".to_string()),
        error_name: None,
        reply_serial: None,
        destination: Some("org.freedesktop.DBus".to_string()),
        sender: None,
        signature: None,
        body: Vec::new(),
    })
}

/// Builds a `METHOD_CALL` to `org.freedesktop.DBus.RequestName(name, flags)`.
pub fn build_request_name_call(serial: u32, name: &str, flags: u32) -> Vec<u8> {
    let mut body = encode_dbus_string(name);
    pad_vec_to(&mut body, 4);
    body.extend_from_slice(&flags.to_le_bytes());

    encode_dbus_message(&DbusMessage {
        endian: DBUS_ENDIAN_LE,
        msg_type: DBUS_MSG_METHOD_CALL,
        flags: 0,
        version: DBUS_PROTO_VERSION,
        serial,
        path: Some("/org/freedesktop/DBus".to_string()),
        interface: Some("org.freedesktop.DBus".to_string()),
        member: Some("RequestName".to_string()),
        error_name: None,
        reply_serial: None,
        destination: Some("org.freedesktop.DBus".to_string()),
        sender: None,
        signature: Some("su".to_string()),
        body,
    })
}

/// Builds a `METHOD_RETURN` message for a given `reply_serial`.
pub fn build_method_return(
    reply_serial: u32,
    serial: u32,
    destination: Option<&str>,
    sender: Option<&str>,
    signature: Option<&str>,
    body: Vec<u8>,
) -> Vec<u8> {
    encode_dbus_message(&DbusMessage {
        endian: DBUS_ENDIAN_LE,
        msg_type: DBUS_MSG_METHOD_RETURN,
        flags: DBUS_FLAG_NO_REPLY_EXPECTED,
        version: DBUS_PROTO_VERSION,
        serial,
        path: None,
        interface: None,
        member: None,
        error_name: None,
        reply_serial: Some(reply_serial),
        destination: destination.map(|s| s.to_string()),
        sender: sender.map(|s| s.to_string()),
        signature: signature.map(|s| s.to_string()),
        body,
    })
}

/// Builds a `NameAcquired` signal emitted by `org.freedesktop.DBus` in standalone bus server mode.
pub fn build_name_acquired_signal(serial: u32, destination: &str, owned_name: &str) -> Vec<u8> {
    encode_dbus_message(&DbusMessage {
        endian: DBUS_ENDIAN_LE,
        msg_type: DBUS_MSG_SIGNAL,
        flags: DBUS_FLAG_NO_REPLY_EXPECTED,
        version: DBUS_PROTO_VERSION,
        serial,
        path: Some("/org/freedesktop/DBus".to_string()),
        interface: Some("org.freedesktop.DBus".to_string()),
        member: Some("NameAcquired".to_string()),
        error_name: None,
        reply_serial: None,
        destination: Some(destination.to_string()),
        sender: Some("org.freedesktop.DBus".to_string()),
        signature: Some("s".to_string()),
        body: encode_dbus_string(owned_name),
    })
}

/// Processes an incoming `DbusMessage` and produces the corresponding `METHOD_RETURN` (and any
/// bus signals when operating as a standalone system bus server).
///
/// Handles:
/// - `GetManagedObjects` -> `a{oa{sa{sv}}}` (`org.freedesktop.DBus.ObjectManager`)
/// - `GetPCMs` -> `a{oa{sv}}` (`org.bluealsa.Manager1`)
/// - `GetAll` -> `a{sv}` (`org.freedesktop.DBus.Properties`)
/// - `Get` -> `v` (`org.freedesktop.DBus.Properties`)
/// - `Introspect` -> `s` (`org.freedesktop.DBus.Introspectable`)
/// - `org.freedesktop.DBus` bus management calls (`Hello`, `RequestName`, `GetNameOwner`,
///   `NameHasOwner`, `StartServiceByName`, `ListNames`, `AddMatch`, `RemoveMatch`, `GetId`)
/// - Any other `METHOD_CALL` -> valid empty `METHOD_RETURN`
pub fn handle_incoming_dbus_message(
    msg: &DbusMessage,
    next_serial: &mut u32,
    standalone_bus_mode: bool,
    client_unique_name: &str,
) -> Vec<Vec<u8>> {
    if msg.msg_type != DBUS_MSG_METHOD_CALL || (msg.flags & DBUS_FLAG_NO_REPLY_EXPECTED) != 0 {
        return Vec::new();
    }

    let dst_storage;
    let reply_dst = if let Some(ref s) = msg.sender {
        Some(s.as_str())
    } else if standalone_bus_mode {
        dst_storage = client_unique_name.to_string();
        Some(dst_storage.as_str())
    } else {
        None
    };

    let reply_sender = if standalone_bus_mode {
        Some(msg.destination.as_deref().unwrap_or("org.freedesktop.DBus"))
    } else {
        None
    };

    let mut alloc_serial = || {
        let s = *next_serial;
        *next_serial = next_serial.wrapping_add(1).max(1);
        s
    };

    let iface = msg.interface.as_deref().unwrap_or("");
    let member = msg.member.as_deref().unwrap_or("");

    if iface == "org.freedesktop.DBus"
        || msg.destination.as_deref() == Some("org.freedesktop.DBus")
    {
        match member {
            "Hello" => {
                let ret = build_method_return(
                    msg.serial,
                    alloc_serial(),
                    reply_dst,
                    Some("org.freedesktop.DBus"),
                    Some("s"),
                    encode_dbus_string(client_unique_name),
                );
                let sig = build_name_acquired_signal(
                    alloc_serial(),
                    reply_dst.unwrap_or(client_unique_name),
                    client_unique_name,
                );
                return vec![ret, sig];
            }
            "RequestName" => {
                let requested = extract_leading_strings(&msg.body, msg.endian, 1)
                    .into_iter()
                    .next()
                    .unwrap_or_default();
                let mut frames = Vec::with_capacity(2);
                if standalone_bus_mode && !requested.is_empty() {
                    frames.push(build_name_acquired_signal(
                        alloc_serial(),
                        reply_dst.unwrap_or(client_unique_name),
                        &requested,
                    ));
                }
                frames.push(build_method_return(
                    msg.serial,
                    alloc_serial(),
                    reply_dst,
                    Some("org.freedesktop.DBus"),
                    Some("u"),
                    DBUS_REQUEST_NAME_REPLY_PRIMARY_OWNER.to_le_bytes().to_vec(),
                ));
                return frames;
            }
            "ReleaseName" => {
                return vec![build_method_return(
                    msg.serial,
                    alloc_serial(),
                    reply_dst,
                    Some("org.freedesktop.DBus"),
                    Some("u"),
                    1u32.to_le_bytes().to_vec(),
                )];
            }
            "StartServiceByName" => {
                return vec![build_method_return(
                    msg.serial,
                    alloc_serial(),
                    reply_dst,
                    Some("org.freedesktop.DBus"),
                    Some("u"),
                    DBUS_START_REPLY_ALREADY_RUNNING.to_le_bytes().to_vec(),
                )];
            }
            "NameHasOwner" => {
                return vec![build_method_return(
                    msg.serial,
                    alloc_serial(),
                    reply_dst,
                    Some("org.freedesktop.DBus"),
                    Some("b"),
                    1u32.to_le_bytes().to_vec(),
                )];
            }
            "GetNameOwner" => {
                return vec![build_method_return(
                    msg.serial,
                    alloc_serial(),
                    reply_dst,
                    Some("org.freedesktop.DBus"),
                    Some("s"),
                    encode_dbus_string(":1.1"),
                )];
            }
            "GetId" => {
                return vec![build_method_return(
                    msg.serial,
                    alloc_serial(),
                    reply_dst,
                    Some("org.freedesktop.DBus"),
                    Some("s"),
                    encode_dbus_string(SERVER_GUID_HEX),
                )];
            }
            "GetConnectionUnixUser" => {
                return vec![build_method_return(
                    msg.serial,
                    alloc_serial(),
                    reply_dst,
                    Some("org.freedesktop.DBus"),
                    Some("u"),
                    0u32.to_le_bytes().to_vec(),
                )];
            }
            "GetConnectionUnixProcessID" => {
                return vec![build_method_return(
                    msg.serial,
                    alloc_serial(),
                    reply_dst,
                    Some("org.freedesktop.DBus"),
                    Some("u"),
                    std::process::id().to_le_bytes().to_vec(),
                )];
            }
            "ListNames" | "ListActivatableNames" => {
                return vec![build_method_return(
                    msg.serial,
                    alloc_serial(),
                    reply_dst,
                    Some("org.freedesktop.DBus"),
                    Some("as"),
                    encode_dbus_string_array(&[
                        "org.freedesktop.DBus",
                        "org.bluez",
                        "org.bluealsa",
                        ":1.1",
                    ]),
                )];
            }
            "AddMatch" | "RemoveMatch" | "UpdateActivationEnvironment" => {
                return vec![build_method_return(
                    msg.serial,
                    alloc_serial(),
                    reply_dst,
                    Some("org.freedesktop.DBus"),
                    None,
                    Vec::new(),
                )];
            }
            _ => {}
        }
    }

    let (sig, body) = match member {
        "GetManagedObjects" => (Some("a{oa{sa{sv}}}"), encode_empty_dict_array()),
        "GetPCMs" => (Some("a{oa{sv}}"), encode_empty_dict_array()),
        "GetAll" => (Some("a{sv}"), encode_empty_dict_array()),
        "Get" => (Some("v"), encode_property_get_variant(&msg.body, msg.endian)),
        "Introspect" => (Some("s"), encode_dbus_string("<node/>")),
        "GetMachineId" => (Some("s"), encode_dbus_string(SERVER_GUID_HEX)),
        _ => (None, Vec::new()),
    };

    vec![build_method_return(
        msg.serial,
        alloc_serial(),
        reply_dst,
        reply_sender,
        sig,
        body,
    )]
}

fn read_crlf_line<R: Read>(reader: &mut R) -> io::Result<String> {
    let mut line = Vec::with_capacity(64);
    let mut byte = [0u8; 1];
    loop {
        reader.read_exact(&mut byte)?;
        if byte[0] == 0 && line.is_empty() {
            // Ignore initial SASL NUL byte (`\0`).
            continue;
        }
        line.push(byte[0]);
        if byte[0] == b'\n' || line.len() >= 1024 {
            break;
        }
    }
    Ok(String::from_utf8_lossy(&line).trim().to_string())
}

/// Performs the D-Bus SASL client authentication handshake (`\0AUTH EXTERNAL 30\r\n` -> `OK` -> `BEGIN\r\n`).
pub fn client_sasl_handshake<S: Read + Write>(stream: &mut S) -> io::Result<()> {
    stream.write_all(b"\0AUTH EXTERNAL 30\r\n")?;
    stream.flush()?;

    let mut line = read_crlf_line(stream)?;
    if line.starts_with("DATA") {
        stream.write_all(b"DATA 30\r\n")?;
        stream.flush()?;
        line = read_crlf_line(stream)?;
    }
    if line.starts_with("REJECTED") {
        stream.write_all(b"AUTH ANONYMOUS\r\n")?;
        stream.flush()?;
        line = read_crlf_line(stream)?;
    }
    if !line.starts_with("OK ") {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("D-Bus SASL handshake rejected: {}", line),
        ));
    }

    stream.write_all(b"BEGIN\r\n")?;
    stream.flush()?;
    Ok(())
}

/// Performs the D-Bus SASL server authentication handshake until the client sends `BEGIN\r\n`.
/// Reads one byte at a time so pipelined binary D-Bus frames immediately following `BEGIN\r\n`
/// remain intact in the stream.
pub fn server_sasl_handshake<S: Read + Write>(stream: &mut S) -> io::Result<()> {
    loop {
        let cmd = read_crlf_line(stream)?;
        if cmd == "BEGIN" {
            return Ok(());
        } else if cmd == "AUTH EXTERNAL" {
            stream.write_all(b"DATA\r\n")?;
            stream.flush()?;
        } else if cmd.starts_with("AUTH EXTERNAL ")
            || cmd.starts_with("AUTH ANONYMOUS")
            || cmd.starts_with("DATA")
        {
            stream.write_all(format!("OK {}\r\n", SERVER_GUID_HEX).as_bytes())?;
            stream.flush()?;
        } else if cmd.starts_with("NEGOTIATE_UNIX_FD") {
            stream.write_all(b"AGREE_UNIX_FD\r\n")?;
            stream.flush()?;
        } else if cmd.is_empty() {
            continue;
        } else {
            stream.write_all(b"REJECTED EXTERNAL ANONYMOUS\r\n")?;
            stream.flush()?;
        }
    }
}

/// Connects to an active `dbus-daemon` on `stream`, completes SASL authentication, sends `Hello`,
/// claims `"org.bluez"` and `"org.bluealsa"`, and serves all incoming `METHOD_CALL` messages.
pub fn serve_bus_client_session<S: Read + Write>(
    stream: &mut S,
    ready_flag: Option<&AtomicBool>,
) -> io::Result<()> {
    client_sasl_handshake(stream)?;

    let mut next_serial = 1u32;
    let hello = build_hello_call(next_serial);
    next_serial += 1;
    let req_bluez = build_request_name_call(next_serial, "org.bluez", DBUS_NAME_FLAGS_CLAIM);
    next_serial += 1;
    let req_bluealsa = build_request_name_call(next_serial, "org.bluealsa", DBUS_NAME_FLAGS_CLAIM);
    next_serial += 1;

    stream.write_all(&hello)?;
    stream.write_all(&req_bluez)?;
    stream.write_all(&req_bluealsa)?;
    stream.flush()?;

    loop {
        let msg = read_dbus_message(stream)?;
        if msg.msg_type == DBUS_MSG_METHOD_RETURN {
            if let Some(flag) = ready_flag {
                if msg.reply_serial == Some(2) || msg.reply_serial == Some(3) {
                    flag.store(true, Ordering::SeqCst);
                }
            }
        }
        let replies = handle_incoming_dbus_message(&msg, &mut next_serial, false, ":1.1");
        if !replies.is_empty() {
            for frame in replies {
                stream.write_all(&frame)?;
            }
            stream.flush()?;
        }
    }
}

/// Handles a single client connection to our standalone D-Bus socket server fallback.
pub fn serve_standalone_bus_peer<S: Read + Write>(
    stream: &mut S,
    peer_id: u32,
) -> io::Result<()> {
    server_sasl_handshake(stream)?;
    let unique_name = format!(":1.{}", peer_id.max(2));
    let mut next_serial = 1u32;
    loop {
        let msg = read_dbus_message(stream)?;
        let replies = handle_incoming_dbus_message(&msg, &mut next_serial, true, &unique_name);
        if !replies.is_empty() {
            for frame in replies {
                stream.write_all(&frame)?;
            }
            stream.flush()?;
        }
    }
}

/// Starts `dbus-daemon --system` if available in the guest image and not already running.
pub fn start_system_dbus_daemon_if_needed(board_slug: &str) {
    if crate::runtime_env::is_process_running("dbus-daemon") {
        // Send SIGHUP so an already-running dbus-daemon reloads `/etc/dbus-1/system.d/kuyase-allow-all.conf`.
        let _ = Command::new("killall")
            .args(["-HUP", "dbus-daemon"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        return;
    }

    let _ = fs::remove_file("/run/dbus/pid");
    let _ = fs::remove_file("/var/run/dbus/pid");

    // On `orion` (Station 3), the stock 64-bit `/usr/bin/dbus-daemon` faults on this kernel;
    // prefer the 32-bit ARMhf `dbus-daemon` from the installed AirPlay package if present.
    if board_slug == "orion" {
        if let Ok(entries) = fs::read_dir("/data/yam/pkgs/station/airplay") {
            for entry in entries.flatten() {
                let ap_dir = entry.path().join("share/luna/plugins/station.airplay");
                let ld = ap_dir.join("lib/ld-linux-armhf.so.3");
                let bin = ap_dir.join("bin/dbus-daemon");
                let lib_dir = ap_dir.join("lib");
                if ld.exists() && bin.exists() {
                    let _ = Command::new(&ld)
                        .arg("--library-path")
                        .arg(&lib_dir)
                        .arg(&bin)
                        .arg("--config-file=/tmp/kuyase-bus.conf")
                        .arg("--fork")
                        .stdin(Stdio::null())
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .status();
                    break;
                }
            }
        }
    }

    if !crate::runtime_env::is_process_running("dbus-daemon")
        && board_slug != "orion"
        && Path::new("/usr/bin/dbus-daemon").exists()
    {
        let ok = Command::new("/usr/bin/dbus-daemon")
            .args(["--system", "--fork"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok && Path::new("/tmp/kuyase-bus.conf").exists() {
            let _ = Command::new("/usr/bin/dbus-daemon")
                .args(["--config-file=/tmp/kuyase-bus.conf", "--fork"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }

    // Wait briefly for `/run/dbus/system_bus_socket` to appear when `dbus-daemon` is starting.
    if crate::runtime_env::is_process_running("dbus-daemon") {
        let deadline = Instant::now() + Duration::from_millis(300);
        while Instant::now() < deadline {
            if Path::new(SYSTEM_BUS_SOCKET_PATH).exists() {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}

#[cfg(unix)]
fn run_dbus_responder_loop(ready: Arc<AtomicBool>) {
    use std::os::unix::net::{UnixListener, UnixStream};

    let mut next_peer_id = 2u32;
    loop {
        if let Ok(mut stream) = UnixStream::connect(SYSTEM_BUS_SOCKET_PATH) {
            let _ = serve_bus_client_session(&mut stream, Some(&ready));
            thread::sleep(Duration::from_millis(150));
            continue;
        }

        // If `dbus-daemon` is not running or unavailable (e.g. on `orion` without AirPlay),
        // bind `/run/dbus/system_bus_socket` directly as a standalone system bus responder.
        if !crate::runtime_env::is_process_running("dbus-daemon") {
            let _ = fs::create_dir_all("/run/dbus");
            let _ = fs::remove_file(SYSTEM_BUS_SOCKET_PATH);
            if let Ok(listener) = UnixListener::bind(SYSTEM_BUS_SOCKET_PATH) {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = fs::set_permissions(
                        SYSTEM_BUS_SOCKET_PATH,
                        fs::Permissions::from_mode(0o777),
                    );
                }
                ready.store(true, Ordering::SeqCst);
                for incoming in listener.incoming() {
                    match incoming {
                        Ok(mut peer) => {
                            let pid = next_peer_id;
                            next_peer_id = next_peer_id.wrapping_add(1).max(2);
                            thread::spawn(move || {
                                let _ = serve_standalone_bus_peer(&mut peer, pid);
                            });
                        }
                        Err(_) => break,
                    }
                }
            }
        }

        thread::sleep(Duration::from_millis(150));
    }
}

#[cfg(not(unix))]
fn run_dbus_responder_loop(ready: Arc<AtomicBool>) {
    ready.store(true, Ordering::SeqCst);
}

/// Runs the `org.bluez` / `org.bluealsa` D-Bus responder in the foreground (when invoked via
/// `qemu-hw-bridge --dbus-stub` from `/usr/share/dbus-1/system-services/org.bluez.service`).
pub fn run_dbus_stub_applet() -> ! {
    let ready = Arc::new(AtomicBool::new(false));
    run_dbus_responder_loop(ready);
    loop {
        thread::sleep(Duration::from_secs(3600));
    }
}

/// Ensures `dbus-daemon --system` and the background `org.bluez` + `org.bluealsa` D-Bus
/// wire-protocol responder are active on `/run/dbus/system_bus_socket` when `Full Mode` is enabled.
pub fn ensure_system_dbus_and_bluez_stub(board_slug: &str, full_mode: bool) {
    if !full_mode {
        return;
    }

    start_system_dbus_daemon_if_needed(board_slug);

    if DBUS_STUB_STARTED
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
    {
        let ready = Arc::new(AtomicBool::new(false));
        let ready_clone = ready.clone();
        thread::spawn(move || {
            run_dbus_responder_loop(ready_clone);
        });

        let deadline = Instant::now() + Duration::from_millis(350);
        while Instant::now() < deadline {
            if ready.load(Ordering::SeqCst) {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_encode_and_parse_hello_and_request_name() {
        let hello_bytes = build_hello_call(1);
        assert_eq!(hello_bytes.len() % 8, 0);
        let parsed_hello = parse_dbus_message(&hello_bytes).unwrap();
        assert_eq!(parsed_hello.msg_type, DBUS_MSG_METHOD_CALL);
        assert_eq!(parsed_hello.serial, 1);
        assert_eq!(parsed_hello.path.as_deref(), Some("/org/freedesktop/DBus"));
        assert_eq!(
            parsed_hello.interface.as_deref(),
            Some("org.freedesktop.DBus")
        );
        assert_eq!(parsed_hello.member.as_deref(), Some("Hello"));
        assert_eq!(
            parsed_hello.destination.as_deref(),
            Some("org.freedesktop.DBus")
        );
        assert!(parsed_hello.body.is_empty());

        let req_bytes = build_request_name_call(2, "org.bluez", DBUS_NAME_FLAGS_CLAIM);
        let parsed_req = parse_dbus_message(&req_bytes).unwrap();
        assert_eq!(parsed_req.msg_type, DBUS_MSG_METHOD_CALL);
        assert_eq!(parsed_req.serial, 2);
        assert_eq!(parsed_req.member.as_deref(), Some("RequestName"));
        assert_eq!(parsed_req.signature.as_deref(), Some("su"));
        assert_eq!(
            extract_leading_strings(&parsed_req.body, parsed_req.endian, 1),
            vec!["org.bluez".to_string()]
        );
    }

    #[test]
    fn test_bluez_and_bluealsa_method_return_signatures_and_alignment() {
        let mut next_serial = 10u32;

        // 1. org.freedesktop.DBus.ObjectManager.GetManagedObjects -> a{oa{sa{sv}}}
        let get_managed = DbusMessage {
            endian: DBUS_ENDIAN_LE,
            msg_type: DBUS_MSG_METHOD_CALL,
            flags: 0,
            version: DBUS_PROTO_VERSION,
            serial: 42,
            path: Some("/".to_string()),
            interface: Some("org.freedesktop.DBus.ObjectManager".to_string()),
            member: Some("GetManagedObjects".to_string()),
            error_name: None,
            reply_serial: None,
            destination: Some("org.bluez".to_string()),
            sender: Some(":1.5".to_string()),
            signature: None,
            body: Vec::new(),
        };
        let replies = handle_incoming_dbus_message(&get_managed, &mut next_serial, false, ":1.5");
        assert_eq!(replies.len(), 1);
        let ret = parse_dbus_message(&replies[0]).unwrap();
        assert_eq!(ret.msg_type, DBUS_MSG_METHOD_RETURN);
        assert_eq!(ret.reply_serial, Some(42));
        assert_eq!(ret.destination.as_deref(), Some(":1.5"));
        assert_eq!(ret.signature.as_deref(), Some("a{oa{sa{sv}}}"));
        // Empty array of 8-byte aligned DICT_ENTRY must be 8 zero bytes (4B len=0 + 4B align pad).
        assert_eq!(ret.body, vec![0u8; 8]);

        // 2. org.bluealsa.Manager1.GetPCMs -> a{oa{sv}}
        let get_pcms = DbusMessage {
            member: Some("GetPCMs".to_string()),
            interface: Some("org.bluealsa.Manager1".to_string()),
            destination: Some("org.bluealsa".to_string()),
            serial: 43,
            ..get_managed.clone()
        };
        let replies = handle_incoming_dbus_message(&get_pcms, &mut next_serial, false, ":1.5");
        let ret = parse_dbus_message(&replies[0]).unwrap();
        assert_eq!(ret.reply_serial, Some(43));
        assert_eq!(ret.signature.as_deref(), Some("a{oa{sv}}"));
        assert_eq!(ret.body, vec![0u8; 8]);

        // 3. org.freedesktop.DBus.Properties.GetAll -> a{sv}
        let get_all = DbusMessage {
            member: Some("GetAll".to_string()),
            interface: Some("org.freedesktop.DBus.Properties".to_string()),
            serial: 44,
            body: encode_dbus_string("org.bluez.Adapter1"),
            ..get_managed.clone()
        };
        let replies = handle_incoming_dbus_message(&get_all, &mut next_serial, false, ":1.5");
        let ret = parse_dbus_message(&replies[0]).unwrap();
        assert_eq!(ret.reply_serial, Some(44));
        assert_eq!(ret.signature.as_deref(), Some("a{sv}"));
        assert_eq!(ret.body, vec![0u8; 8]);

        // 4. org.freedesktop.DBus.Properties.Get("org.bluez.Adapter1", "Powered") -> v (boolean)
        let mut get_prop_body = encode_dbus_string("org.bluez.Adapter1");
        pad_vec_to(&mut get_prop_body, 4);
        get_prop_body.extend_from_slice(&encode_dbus_string("Powered"));
        let get_prop = DbusMessage {
            member: Some("Get".to_string()),
            interface: Some("org.freedesktop.DBus.Properties".to_string()),
            signature: Some("ss".to_string()),
            serial: 45,
            body: get_prop_body,
            ..get_managed.clone()
        };
        let replies = handle_incoming_dbus_message(&get_prop, &mut next_serial, false, ":1.5");
        let ret = parse_dbus_message(&replies[0]).unwrap();
        assert_eq!(ret.reply_serial, Some(45));
        assert_eq!(ret.signature.as_deref(), Some("v"));
        assert_eq!(ret.body, vec![1, b'b', 0, 0, 0, 0, 0, 0]);

        // 5. Arbitrary method call (e.g. RegisterAgent) -> valid empty METHOD_RETURN
        let reg_agent = DbusMessage {
            member: Some("RegisterAgent".to_string()),
            interface: Some("org.bluez.AgentManager1".to_string()),
            serial: 46,
            ..get_managed
        };
        let replies = handle_incoming_dbus_message(&reg_agent, &mut next_serial, false, ":1.5");
        let ret = parse_dbus_message(&replies[0]).unwrap();
        assert_eq!(ret.msg_type, DBUS_MSG_METHOD_RETURN);
        assert_eq!(ret.reply_serial, Some(46));
        assert_eq!(ret.signature, None);
        assert!(ret.body.is_empty());
    }

    #[test]
    fn test_sasl_and_stream_framing_roundtrip() {
        // Verify SASL server handshake stops cleanly at BEGIN\r\n without consuming binary frames.
        let mut wire = Vec::new();
        wire.extend_from_slice(b"\0AUTH EXTERNAL 30\r\nNEGOTIATE_UNIX_FD\r\nBEGIN\r\n");
        let hello_frame = build_hello_call(7);
        wire.extend_from_slice(&hello_frame);

        struct DuplexBuffer {
            rx: Cursor<Vec<u8>>,
            tx: Vec<u8>,
        }
        impl Read for DuplexBuffer {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                self.rx.read(buf)
            }
        }
        impl Write for DuplexBuffer {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                self.tx.extend_from_slice(buf);
                Ok(buf.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        let mut duplex = DuplexBuffer {
            rx: Cursor::new(wire),
            tx: Vec::new(),
        };
        server_sasl_handshake(&mut duplex).unwrap();
        let tx_str = String::from_utf8_lossy(&duplex.tx);
        assert!(tx_str.contains("OK 0123456789abcdef0123456789abcdef\r\n"));
        assert!(tx_str.contains("AGREE_UNIX_FD\r\n"));

        // Immediately read the pipelined binary Hello message after BEGIN\r\n.
        let msg = read_dbus_message(&mut duplex).unwrap();
        assert_eq!(msg.serial, 7);
        assert_eq!(msg.member.as_deref(), Some("Hello"));

        // Verify client_sasl_handshake + serve_bus_client_session + serve_standalone_bus_peer
        // end-to-end over a scripted stream.
        let mut server_reply_stream = Vec::new();
        server_reply_stream
            .extend_from_slice(b"OK 0123456789abcdef0123456789abcdef\r\n");
        // Reply to Hello(1), RequestName("org.bluez")(2), RequestName("org.bluealsa")(3),
        // then send a GetManagedObjects METHOD_CALL(10) to the bus client.
        server_reply_stream.extend_from_slice(&build_method_return(
            1,
            1,
            Some(":1.1"),
            Some("org.freedesktop.DBus"),
            Some("s"),
            encode_dbus_string(":1.1"),
        ));
        server_reply_stream.extend_from_slice(&build_method_return(
            2,
            2,
            Some(":1.1"),
            Some("org.freedesktop.DBus"),
            Some("u"),
            1u32.to_le_bytes().to_vec(),
        ));
        server_reply_stream.extend_from_slice(&build_method_return(
            3,
            3,
            Some(":1.1"),
            Some("org.freedesktop.DBus"),
            Some("u"),
            1u32.to_le_bytes().to_vec(),
        ));
        server_reply_stream.extend_from_slice(&encode_dbus_message(&DbusMessage {
            endian: DBUS_ENDIAN_LE,
            msg_type: DBUS_MSG_METHOD_CALL,
            flags: 0,
            version: DBUS_PROTO_VERSION,
            serial: 10,
            path: Some("/".to_string()),
            interface: Some("org.freedesktop.DBus.ObjectManager".to_string()),
            member: Some("GetManagedObjects".to_string()),
            error_name: None,
            reply_serial: None,
            destination: Some("org.bluez".to_string()),
            sender: Some(":1.9".to_string()),
            signature: None,
            body: Vec::new(),
        }));

        let mut client_duplex = DuplexBuffer {
            rx: Cursor::new(server_reply_stream),
            tx: Vec::new(),
        };
        let ready = AtomicBool::new(false);
        let _ = serve_bus_client_session(&mut client_duplex, Some(&ready));
        assert!(ready.load(Ordering::SeqCst));

        // Verify standalone bus peer session with SASL + Hello + GetPCMs.
        let mut peer_rx = Vec::new();
        peer_rx.extend_from_slice(b"\0AUTH EXTERNAL 30\r\nBEGIN\r\n");
        peer_rx.extend_from_slice(&build_hello_call(1));
        peer_rx.extend_from_slice(&encode_dbus_message(&DbusMessage {
            endian: DBUS_ENDIAN_LE,
            msg_type: DBUS_MSG_METHOD_CALL,
            flags: 0,
            version: DBUS_PROTO_VERSION,
            serial: 2,
            path: Some("/org/bluealsa".to_string()),
            interface: Some("org.bluealsa.Manager1".to_string()),
            member: Some("GetPCMs".to_string()),
            error_name: None,
            reply_serial: None,
            destination: Some("org.bluealsa".to_string()),
            sender: None,
            signature: None,
            body: Vec::new(),
        }));
        let mut peer_duplex = DuplexBuffer {
            rx: Cursor::new(peer_rx),
            tx: Vec::new(),
        };
        let _ = serve_standalone_bus_peer(&mut peer_duplex, 2);
        assert!(peer_duplex.tx.len() > 64);
    }
}
