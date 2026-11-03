use super::decompress_zlib_or_deflate;
use crate::image_builder::RootfsTree;
use std::collections::BTreeMap;

pub const UBIFS_NODE_MAGIC: u32 = 0x0610_1831;

pub(crate) const UBIFS_INO_NODE: u8 = 0;
pub(crate) const UBIFS_DATA_NODE: u8 = 1;
pub(crate) const UBIFS_DENT_NODE: u8 = 2;
const UBIFS_PAD_NODE: u8 = 5;
const UBIFS_MAX_NODE_TYPE: u8 = 13;

pub(crate) const UBIFS_COMPR_NONE: u16 = 0;
pub(crate) const UBIFS_COMPR_LZO: u16 = 1;
const UBIFS_COMPR_ZLIB: u16 = 2;

const S_IFMT: u16 = 0xF000;
pub(crate) const S_IFDIR: u16 = 0x4000;
pub(crate) const S_IFREG: u16 = 0x8000;
pub(crate) const S_IFLNK: u16 = 0xA000;

#[derive(Clone, Debug)]
struct UbifsIno {
    sqnum: u64,
    size: u64,
    nlink: u32,
    uid: u16,
    gid: u16,
    mode: u16,
    data: Vec<u8>,
}

#[derive(Clone, Debug)]
struct UbifsDent {
    sqnum: u64,
    target_inum: u32,
    entry_type: u8,
    name: String,
}

#[derive(Clone, Debug)]
struct UbifsDataBlock {
    sqnum: u64,
    bytes: Vec<u8>,
}

/// Pure-Rust zero-dependency decompressor for `LZO1X-1` (`UBIFS_COMPR_LZO = 1`).
///
/// The Linux kernel UBIFS filesystem (`fs/ubifs/compress.c`) compresses 4 KiB
/// `UBIFS_DATA_NODE` payloads with `LZO1X-1` (`lib/lzo/lzo1x_decompress_safe.c`)
/// by default on Yandex smart speaker NAND/UBI firmware images (`rootfs.ubifs`).
///
/// ### `LZO1X-1` Bitstream State Machine
/// 1. **Initial byte (`src[0]`)**:
///    - If `src[0] > 17`, encodes an unaligned initial literal run of `t = src[0] - 17`
///      bytes (`1..=238`). After copying `t` bytes:
///      - If `t < 4`, the next instruction byte `t = src[ip]` enters `Match` directly.
///      - If `t >= 4`, execution transitions to `FirstLiteralRun`.
///    - If `src[0] <= 17`, execution enters `OuterLoop` directly (re-reading `src[0]`).
/// 2. **`OuterLoop`**:
///    - Reads `t = src[ip]; ip += 1`.
///    - If `t >= 16`, transitions directly to `Match`.
///    - If `t < 16`, decodes a literal run of `t + 3` bytes (when `t == 0`, extended
///      by `255` for each `0x00` byte plus `15 + next_byte`), copies the literals to
///      `out`, and transitions to `FirstLiteralRun`.
/// 3. **`FirstLiteralRun`**:
///    - Reads `t = src[ip]; ip += 1`.
///    - If `t >= 16`, transitions to `Match`.
///    - If `t < 16`, decodes a 3-byte match at distance
///      `1 + 0x0800 + (t >> 2) + (src[ip] << 2)` and transitions to `MatchDone`.
/// 4. **`Match` (LZ77 lookbehind copy & EOF dispatcher)**:
///    - `t >= 64` (**M2 match**): length `(t >> 5) + 1` (`3..=8` B), distance
///      `1 + ((t >> 2) & 7) + (src[ip] << 3)` (`1..=2048`).
///    - `32 <= t < 64` (**M3 match**): length `(t & 31) + 2` (if `(t & 31) == 0`,
///      extended by `255` per `0x00` byte + `31 + next_byte`), 14-bit distance
///      `1 + (le16 >> 2)` (`1..=16384`).
///    - `16 <= t < 32` (**M4 match / EOF**): length `(t & 7) + 2` (if `(t & 7) == 0`,
///      extended by `255` per `0x00` byte + `7 + next_byte`), distance offset
///      `((t & 8) << 11) + (le16 >> 2)`.
///      - When this offset is `0` (canonical 3-byte EOF sequence `0x11 0x00 0x00`),
///        the stream is complete and returns `Ok(out)`.
///      - Otherwise adds `0x4000` (`16384`) for a far match in `16384..=49151`.
///    - `t < 16` (**M1 match**): length `2` B, distance
///      `1 + (t >> 2) + (src[ip] << 2)` (`1..=1024`).
/// 5. **`MatchDone` (Trailing literal count in low 2 bits)**:
///    - Reads the low 2 bits of the preceding match instruction (`src[ip - 2] & 3`).
///    - If `0`, returns to `OuterLoop`.
///    - If `1..=3`, copies `t` trailing literal bytes, reads `t = src[ip]`, and
///      continues directly in `Match`.
pub(crate) fn decompress_lzo1x(src: &[u8], expected_len: usize) -> Result<Vec<u8>, String> {
    if src.is_empty() {
        return if expected_len == 0 {
            Ok(Vec::new())
        } else {
            Err("empty LZO1X input".to_string())
        };
    }

    let mut out = Vec::with_capacity(expected_len.clamp(64, 65536));
    let mut ip = 0usize;
    let mut t: usize;

    enum State {
        OuterLoop,
        FirstLiteralRun,
        Match,
        MatchDone,
    }

    let mut state = if src[0] > 17 {
        t = (src[0] as usize) - 17;
        ip = 1;
        if ip + t > src.len() {
            return Err("LZO1X initial literal run out of bounds".to_string());
        }
        out.extend_from_slice(&src[ip..ip + t]);
        ip += t;
        if t < 4 {
            if ip >= src.len() {
                return Err("LZO1X truncated after short initial literal".to_string());
            }
            t = src[ip] as usize;
            ip += 1;
            State::Match
        } else {
            State::FirstLiteralRun
        }
    } else {
        t = 0;
        State::OuterLoop
    };

    loop {
        match state {
            State::OuterLoop => {
                if ip >= src.len() {
                    break;
                }
                t = src[ip] as usize;
                ip += 1;
                if t >= 16 {
                    state = State::Match;
                    continue;
                }
                if t == 0 {
                    while ip < src.len() && src[ip] == 0 {
                        t = t.checked_add(255).ok_or("LZO1X literal length overflow")?;
                        ip += 1;
                    }
                    if ip >= src.len() {
                        return Err("LZO1X truncated in zero-extended literal run".to_string());
                    }
                    t = t
                        .checked_add(15 + (src[ip] as usize))
                        .ok_or("LZO1X literal length overflow")?;
                    ip += 1;
                }
                let copy_len = t + 3;
                if ip + copy_len > src.len() {
                    return Err("LZO1X literal run exceeds input slice".to_string());
                }
                out.extend_from_slice(&src[ip..ip + copy_len]);
                ip += copy_len;
                state = State::FirstLiteralRun;
            }
            State::FirstLiteralRun => {
                if ip >= src.len() {
                    break;
                }
                t = src[ip] as usize;
                ip += 1;
                if t >= 16 {
                    state = State::Match;
                    continue;
                }
                if ip >= src.len() {
                    return Err("LZO1X truncated in first_literal_run match".to_string());
                }
                let dist = (1 + 0x0800) + (t >> 2) + ((src[ip] as usize) << 2);
                ip += 1;
                let m_pos = out
                    .len()
                    .checked_sub(dist)
                    .ok_or("LZO1X first_literal_run lookbehind underflow")?;
                for i in 0..3 {
                    let b = out[m_pos + i];
                    out.push(b);
                }
                state = State::MatchDone;
            }
            State::Match => {
                let (m_pos, m_len) = if t >= 64 {
                    if ip >= src.len() {
                        return Err("LZO1X truncated in M2 match".to_string());
                    }
                    let dist = 1 + ((t >> 2) & 7) + ((src[ip] as usize) << 3);
                    ip += 1;
                    let m_len = (t >> 5) + 1;
                    let m_pos = out
                        .len()
                        .checked_sub(dist)
                        .ok_or("LZO1X M2 lookbehind underflow")?;
                    (m_pos, m_len)
                } else if t >= 32 {
                    let mut m_len = t & 31;
                    if m_len == 0 {
                        while ip < src.len() && src[ip] == 0 {
                            m_len = m_len
                                .checked_add(255)
                                .ok_or("LZO1X M3 length overflow")?;
                            ip += 1;
                        }
                        if ip >= src.len() {
                            return Err("LZO1X truncated in M3 length extension".to_string());
                        }
                        m_len = m_len
                            .checked_add(31 + (src[ip] as usize))
                            .ok_or("LZO1X M3 length overflow")?;
                        ip += 1;
                    }
                    m_len += 2;
                    if ip + 2 > src.len() {
                        return Err("LZO1X truncated in M3 offset".to_string());
                    }
                    let v = u16::from_le_bytes([src[ip], src[ip + 1]]) as usize;
                    ip += 2;
                    let dist = 1 + (v >> 2);
                    let m_pos = out
                        .len()
                        .checked_sub(dist)
                        .ok_or("LZO1X M3 lookbehind underflow")?;
                    (m_pos, m_len)
                } else if t >= 16 {
                    let mut dist = (t & 8) << 11;
                    let mut m_len = t & 7;
                    if m_len == 0 {
                        while ip < src.len() && src[ip] == 0 {
                            m_len = m_len
                                .checked_add(255)
                                .ok_or("LZO1X M4 length overflow")?;
                            ip += 1;
                        }
                        if ip >= src.len() {
                            return Err("LZO1X truncated in M4 length extension".to_string());
                        }
                        m_len = m_len
                            .checked_add(7 + (src[ip] as usize))
                            .ok_or("LZO1X M4 length overflow")?;
                        ip += 1;
                    }
                    m_len += 2;
                    if ip + 2 > src.len() {
                        return Err("LZO1X truncated in M4 offset".to_string());
                    }
                    let v = u16::from_le_bytes([src[ip], src[ip + 1]]) as usize;
                    ip += 2;
                    dist += v >> 2;
                    if dist == 0 {
                        // Canonical LZO1X EOF marker (0x11 0x00 0x00)
                        break;
                    }
                    dist += 0x4000;
                    let m_pos = out
                        .len()
                        .checked_sub(dist)
                        .ok_or("LZO1X M4 lookbehind underflow")?;
                    (m_pos, m_len)
                } else {
                    if ip >= src.len() {
                        return Err("LZO1X truncated in M1 match".to_string());
                    }
                    let dist = 1 + (t >> 2) + ((src[ip] as usize) << 2);
                    ip += 1;
                    let m_pos = out
                        .len()
                        .checked_sub(dist)
                        .ok_or("LZO1X M1 lookbehind underflow")?;
                    (m_pos, 2)
                };

                for i in 0..m_len {
                    let b = out[m_pos + i];
                    out.push(b);
                }
                state = State::MatchDone;
            }
            State::MatchDone => {
                t = (src[ip - 2] as usize) & 3;
                if t == 0 {
                    state = State::OuterLoop;
                } else {
                    if ip + t > src.len() {
                        return Err("LZO1X trailing literal exceeds input slice".to_string());
                    }
                    out.extend_from_slice(&src[ip..ip + t]);
                    ip += t;
                    if ip >= src.len() {
                        break;
                    }
                    t = src[ip] as usize;
                    ip += 1;
                    state = State::Match;
                }
            }
        }
    }

    if expected_len > 0 && out.len() > expected_len {
        out.truncate(expected_len);
    }
    Ok(out)
}

/// Scans `slice` for the next little-endian `UBIFS_NODE_MAGIC` (`[0x31, 0x18, 0x10, 0x06]`),
/// skipping `0xFF` / `0x00` LEB padding runs without byte-by-byte header decoding.
fn find_next_ubifs_magic(slice: &[u8]) -> Option<usize> {
    let mut offset = 0usize;
    while offset + 4 <= slice.len() {
        let rel = slice[offset..slice.len() - 3]
            .iter()
            .position(|&b| b == 0x31)?;
        let idx = offset + rel;
        if slice[idx + 1] == 0x18 && slice[idx + 2] == 0x10 && slice[idx + 3] == 0x06 {
            return Some(idx);
        }
        offset = idx + 1;
    }
    None
}

pub fn extract_ubifs_into_tree(raw: &[u8]) -> Result<RootfsTree, String> {
    let mut inodes: BTreeMap<u32, UbifsIno> = BTreeMap::new();
    let mut dents: BTreeMap<(u32, String), UbifsDent> = BTreeMap::new();
    let mut data_blocks: BTreeMap<(u32, u32), UbifsDataBlock> = BTreeMap::new();

    let mut pos = 0usize;
    while pos + 24 <= raw.len() {
        let magic = u32::from_le_bytes([raw[pos], raw[pos + 1], raw[pos + 2], raw[pos + 3]]);
        if magic != UBIFS_NODE_MAGIC {
            if let Some(rel) = find_next_ubifs_magic(&raw[pos + 1..]) {
                pos += 1 + rel;
                continue;
            } else {
                break;
            }
        }
        let sqnum = u64::from_le_bytes([
            raw[pos + 8],
            raw[pos + 9],
            raw[pos + 10],
            raw[pos + 11],
            raw[pos + 12],
            raw[pos + 13],
            raw[pos + 14],
            raw[pos + 15],
        ]);
        let node_len =
            u32::from_le_bytes([raw[pos + 16], raw[pos + 17], raw[pos + 18], raw[pos + 19]])
                as usize;
        let node_type = raw[pos + 20];

        if node_len < 24 || pos + node_len > raw.len() || node_type > UBIFS_MAX_NODE_TYPE {
            pos += 4;
            continue;
        }
        let node = &raw[pos..pos + node_len];

        match node_type {
            UBIFS_PAD_NODE if node.len() >= 28 => {
                let pad_len = u32::from_le_bytes([node[24], node[25], node[26], node[27]]) as usize;
                let aligned = (node_len + 7) & !7;
                if let Some(next_pos) = pos.checked_add(aligned).and_then(|p| p.checked_add(pad_len))
                {
                    if next_pos <= raw.len() {
                        pos = next_pos;
                        continue;
                    }
                }
            }
            UBIFS_INO_NODE if node.len() >= 160 => {
                let inum = u32::from_le_bytes([node[24], node[25], node[26], node[27]]);
                let size = u64::from_le_bytes([
                    node[48], node[49], node[50], node[51], node[52], node[53], node[54], node[55],
                ]);
                let nlink = u32::from_le_bytes([node[92], node[93], node[94], node[95]]);
                let uid = u32::from_le_bytes([node[96], node[97], node[98], node[99]]) as u16;
                let gid = u32::from_le_bytes([node[100], node[101], node[102], node[103]]) as u16;
                let mode = u32::from_le_bytes([node[104], node[105], node[106], node[107]]) as u16;
                let data_len =
                    u32::from_le_bytes([node[112], node[113], node[114], node[115]]) as usize;
                let end = (160 + data_len).min(node.len());
                let inline = node[160..end].to_vec();

                let replace = inodes.get(&inum).map(|old| sqnum >= old.sqnum).unwrap_or(true);
                if replace {
                    inodes.insert(
                        inum,
                        UbifsIno {
                            sqnum,
                            size,
                            nlink,
                            uid,
                            gid,
                            mode,
                            data: inline,
                        },
                    );
                }
            }
            UBIFS_DENT_NODE if node.len() >= 56 => {
                let parent_inum = u32::from_le_bytes([node[24], node[25], node[26], node[27]]);
                let target_inum = u64::from_le_bytes([
                    node[40], node[41], node[42], node[43], node[44], node[45], node[46], node[47],
                ]) as u32;
                let entry_type = node[49];
                let nlen = u16::from_le_bytes([node[50], node[51]]) as usize;
                if 56 + nlen <= node.len() && nlen > 0 {
                    let name = String::from_utf8_lossy(&node[56..56 + nlen])
                        .trim_end_matches('\0')
                        .to_string();
                    if !name.is_empty() && name != "." && name != ".." {
                        let key = (parent_inum, name.clone());
                        let replace = dents.get(&key).map(|old| sqnum >= old.sqnum).unwrap_or(true);
                        if replace {
                            dents.insert(
                                key,
                                UbifsDent {
                                    sqnum,
                                    target_inum,
                                    entry_type,
                                    name,
                                },
                            );
                        }
                    }
                }
            }
            UBIFS_DATA_NODE if node.len() >= 48 => {
                let inum = u32::from_le_bytes([node[24], node[25], node[26], node[27]]);
                let k1 = u32::from_le_bytes([node[28], node[29], node[30], node[31]]);
                let block_num = k1 & 0x1FFF_FFFF;
                let uncomp_len =
                    u32::from_le_bytes([node[40], node[41], node[42], node[43]]) as usize;
                let compr_type = u16::from_le_bytes([node[44], node[45]]);
                let payload = &node[48..];
                let replace = data_blocks
                    .get(&(inum, block_num))
                    .map(|old| sqnum >= old.sqnum)
                    .unwrap_or(true);
                if replace {
                    let decoded = match compr_type {
                        UBIFS_COMPR_NONE => payload[..payload.len().min(uncomp_len)].to_vec(),
                        UBIFS_COMPR_LZO => decompress_lzo1x(payload, uncomp_len)?,
                        UBIFS_COMPR_ZLIB => {
                            decompress_zlib_or_deflate(payload).unwrap_or_default()
                        }
                        _ => payload[..payload.len().min(uncomp_len)].to_vec(),
                    };
                    data_blocks.insert(
                        (inum, block_num),
                        UbifsDataBlock {
                            sqnum,
                            bytes: decoded,
                        },
                    );
                }
            }
            _ => {}
        }

        let aligned = (node_len + 7) & !7;
        pos += aligned.max(8);
    }

    if dents.is_empty() && inodes.is_empty() {
        return Err("no valid UBIFS nodes found".to_string());
    }

    let mut children_by_parent: BTreeMap<u32, Vec<UbifsDent>> = BTreeMap::new();
    for ((parent, _), dent) in dents {
        if dent.target_inum != 0 {
            children_by_parent.entry(parent).or_default().push(dent);
        }
    }

    let mut tree = RootfsTree::default();
    let mut stack = vec![(1u32, String::new())];
    while let Some((dir_inum, prefix)) = stack.pop() {
        let Some(children) = children_by_parent.get(&dir_inum) else {
            continue;
        };
        for child in children {
            let rel = if prefix.is_empty() {
                child.name.clone()
            } else {
                format!("{}/{}", prefix, child.name)
            };
            let ino_opt = inodes.get(&child.target_inum);
            if let Some(ino) = ino_opt {
                if ino.nlink == 0 {
                    continue;
                }
            }
            let (fmt, perm, uid, gid) = match ino_opt {
                Some(ino) => (
                    ino.mode & S_IFMT,
                    if (ino.mode & 0o7777) == 0 {
                        0o755
                    } else {
                        ino.mode & 0o7777
                    },
                    ino.uid,
                    ino.gid,
                ),
                None => {
                    let fmt = match child.entry_type {
                        1 => S_IFDIR,
                        2 => S_IFLNK,
                        _ => S_IFREG,
                    };
                    (fmt, 0o755, 0, 0)
                }
            };

            if fmt == S_IFDIR || child.entry_type == 1 {
                tree.insert_dir_with_owner(&rel, perm, uid, gid);
                stack.push((child.target_inum, rel));
            } else if fmt == S_IFLNK || child.entry_type == 2 {
                let target = ino_opt
                    .map(|i| String::from_utf8_lossy(&i.data).to_string())
                    .unwrap_or_default();
                tree.insert_symlink_with_owner(&rel, target, uid, gid);
            } else if fmt == S_IFREG || child.entry_type == 0 {
                let file_size = ino_opt.map(|i| i.size as usize).unwrap_or(0);
                let mut file_bytes = vec![0u8; file_size];
                let num_blks = file_size.div_ceil(4096);
                for blk_idx in 0..num_blks as u32 {
                    if let Some(blk) = data_blocks.get(&(child.target_inum, blk_idx)) {
                        let start = (blk_idx as usize) * 4096;
                        if start < file_size {
                            let take = blk.bytes.len().min(file_size - start);
                            file_bytes[start..start + take].copy_from_slice(&blk.bytes[..take]);
                        }
                    }
                }
                if file_size == 0 {
                    if let Some(blk) = data_blocks.get(&(child.target_inum, 0)) {
                        file_bytes = blk.bytes.clone();
                    }
                }
                tree.insert_file_with_owner(&rel, file_bytes, perm, uid, gid);
            }
        }
    }

    if tree.entries.is_empty() {
        return Err("UBIFS stream contained no extractable files".to_string());
    }
    Ok(tree)
}
