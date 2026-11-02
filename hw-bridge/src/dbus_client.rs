//! Minimal D-Bus client for the AirPlay transport commands.
//!
//! Not every guest image ships `dbus-send`: the older `yandexmini` rootfs has
//! no D-Bus tools at all, and on the others a missing/spawning binary turns a
//! working voice command into a silent failure. The bridge therefore speaks
//! the wire protocol itself: one EXTERNAL handshake and one method call with
//! no arguments. That is all the shairport RemoteControl interface needs.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

const HEADER_SIZE: usize = 16;

fn align_to(buf: &mut Vec<u8>, n: usize) {
    while buf.len() % n != 0 {
        buf.push(0);
    }
}

fn align_up(value: usize, n: usize) -> usize {
    (value + n - 1) / n * n
}

fn put_string(buf: &mut Vec<u8>, value: &str) {
    align_to(buf, 4);
    buf.extend_from_slice(&(value.len() as u32).to_le_bytes());
    buf.extend_from_slice(value.as_bytes());
    buf.push(0);
}

/// Header field: struct { byte code; variant value }, 8-byte aligned. The
/// variant signature is one byte, which is all we need for o/s/u/g.
fn put_field(buf: &mut Vec<u8>, code: u8, signature: u8, value: &str) {
    align_to(buf, 8);
    buf.push(code);
    buf.push(1); // variant signature length
    buf.push(signature);
    buf.push(0); // signature terminator
    put_string(buf, value);
}

fn build_call(destination: &str, path: &str, interface: &str, member: &str) -> Vec<u8> {
    let mut msg = Vec::with_capacity(256);
    msg.push(b'l'); // little endian
    msg.push(1); // METHOD_CALL
    msg.push(0); // flags
    msg.push(1); // protocol version
    msg.extend_from_slice(&0u32.to_le_bytes()); // body length, patched below
    msg.extend_from_slice(&1u32.to_le_bytes()); // serial
    msg.extend_from_slice(&0u32.to_le_bytes()); // header fields length, patched below
    let fields_start = msg.len();
    put_field(&mut msg, 1, b'o', path); // PATH
    put_field(&mut msg, 2, b's', interface); // INTERFACE
    put_field(&mut msg, 3, b's', member); // MEMBER
    put_field(&mut msg, 6, b's', destination); // DESTINATION
    let fields_len = (msg.len() - fields_start) as u32;
    msg[12..16].copy_from_slice(&fields_len.to_le_bytes());
    msg
}

fn read_line(stream: &mut UnixStream) -> Result<String, String> {
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    while line.len() < 512 {
        stream.read_exact(&mut byte).map_err(|e| e.to_string())?;
        line.push(byte[0]);
        if line.ends_with(b"\r\n") {
            break;
        }
    }
    Ok(String::from_utf8_lossy(&line).trim_end().to_string())
}

fn u32_at(bytes: &[u8], offset: usize, endian: u8) -> Option<u32> {
    let raw = bytes.get(offset..offset + 4)?;
    let arr = [raw[0], raw[1], raw[2], raw[3]];
    Some(if endian == b'l' {
        u32::from_le_bytes(arr)
    } else {
        u32::from_be_bytes(arr)
    })
}

/// Walk the reply header fields and return ERROR_NAME (code 4), so a rejected
/// call logs e.g. `org.freedesktop.DBus.Error.ServiceUnknown` instead of a
/// bare "failed".
fn error_name(fields: &[u8]) -> Option<String> {
    let mut off = 0usize;
    while off + 4 <= fields.len() {
        let start = align_up(off, 8);
        if start + 4 > fields.len() {
            return None;
        }
        let code = fields[start];
        let sig_len = fields[start + 1] as usize;
        let sig_start = start + 2;
        let sig = fields.get(sig_start..sig_start + sig_len)?;
        let value_start = align_up(sig_start + sig_len + 1, 4);
        let sig_char = *sig.first()?;
        let value_end = match sig_char {
            b's' | b'o' => {
                let len = u32_at(fields, value_start, b'l')? as usize;
                value_start + 4 + len + 1
            }
            b'g' => {
                let len = *fields.get(value_start)? as usize;
                value_start + 1 + len + 1
            }
            b'u' => value_start + 4,
            _ => return None,
        };
        if code == 4 && (sig_char == b's' || sig_char == b'o') {
            let len = u32_at(fields, value_start, b'l')? as usize;
            let text = fields.get(value_start + 4..value_start + 4 + len)?;
            return Some(String::from_utf8_lossy(text).into_owned());
        }
        if value_end <= start {
            return None;
        }
        off = value_end;
    }
    None
}

/// Send one method call with no arguments and wait for the reply.
pub fn call_no_args(
    bus: &str,
    destination: &str,
    path: &str,
    interface: &str,
    member: &str,
) -> Result<(), String> {
    let mut stream = UnixStream::connect(bus).map_err(|e| format!("connect: {e}"))?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(3)));

    // EXTERNAL auth: the initial response is the uid as hex-encoded ASCII.
    // The bridge runs as root in the guest, so uid 0 -> "30".
    stream.write_all(b"\0").map_err(|e| e.to_string())?;
    stream
        .write_all(b"AUTH EXTERNAL 30\r\n")
        .map_err(|e| e.to_string())?;
    let auth = read_line(&mut stream)?;
    if !auth.starts_with("OK ") {
        return Err(format!("auth rejected: {auth}"));
    }
    stream.write_all(b"BEGIN\r\n").map_err(|e| e.to_string())?;

    stream
        .write_all(&build_call(destination, path, interface, member))
        .map_err(|e| e.to_string())?;

    let mut header = [0u8; HEADER_SIZE];
    stream.read_exact(&mut header).map_err(|e| e.to_string())?;
    let endian = header[0];
    let msg_type = header[1];
    let body_len = u32_at(&header, 4, endian).ok_or("short header")? as usize;
    let fields_len = u32_at(&header, 12, endian).ok_or("short header")? as usize;
    let padding = align_up(HEADER_SIZE + fields_len, 8) - (HEADER_SIZE + fields_len);
    let rest = fields_len + padding + body_len;
    let mut payload = vec![0u8; rest];
    if rest > 0 {
        stream.read_exact(&mut payload).map_err(|e| e.to_string())?;
    }

    match msg_type {
        2 => Ok(()),
        3 => Err(error_name(&payload).unwrap_or_else(|| "error reply".to_string())),
        other => Err(format!("unexpected reply type {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field_codes(msg: &[u8]) -> Vec<u8> {
        let mut codes = Vec::new();
        let fields_len = u32_at(msg, 12, b'l').unwrap() as usize;
        let mut off = HEADER_SIZE;
        let end = HEADER_SIZE + fields_len;
        while off < end {
            off = align_up(off, 8);
            if off >= end {
                break;
            }
            let code = msg[off];
            codes.push(code);
            let sig_len = msg[off + 1] as usize;
            let value_start = align_up(off + 2 + sig_len + 1, 4);
            let len = u32_at(msg, value_start, b'l').unwrap() as usize;
            off = value_start + 4 + len + 1;
        }
        codes
    }

    #[test]
    fn call_has_path_interface_member_destination() {
        let msg = build_call(
            "org.gnome.ShairportSync",
            "/org/gnome/ShairportSync",
            "org.gnome.ShairportSync.RemoteControl",
            "Pause",
        );
        assert_eq!(msg[0], b'l');
        assert_eq!(msg[1], 1, "METHOD_CALL");
        assert_eq!(u32_at(&msg, 4, b'l'), Some(0), "no body");
        assert_eq!(
            u32_at(&msg, 12, b'l'),
            Some((msg.len() - HEADER_SIZE) as u32),
            "header array length"
        );
        assert_eq!(field_codes(&msg), vec![1, 2, 3, 6]);
        let body = String::from_utf8_lossy(&msg);
        assert!(body.contains("/org/gnome/ShairportSync"));
        assert!(body.contains("org.gnome.ShairportSync.RemoteControl"));
        assert!(body.contains("Pause"));
        assert!(body.contains("org.gnome.ShairportSync"));
    }
}
