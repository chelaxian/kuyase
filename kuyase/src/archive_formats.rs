mod squashfs;
mod ubifs;

pub use squashfs::{extract_squashfs_into_tree, SQUASHFS_MAGIC};
pub use ubifs::{extract_ubifs_into_tree, UBIFS_NODE_MAGIC};

use crate::ext_fs::{read_ext2_image, EXT2_SUPER_MAGIC};
use crate::image_builder::{read_tar_from_reader, RootfsTree};
use flate2::read::{DeflateDecoder, GzDecoder, ZlibDecoder};
use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Read};

pub const UBI_EC_MAGIC: &[u8; 4] = b"UBI#";
pub const UBI_VID_MAGIC: &[u8; 4] = b"UBI!";

/// SVR4 ASCII CPIO header magic without CRC (`070701`) and with CRC (`070702`),
/// used by `swupdate` OTA containers (`update.zip` / `.swu`).
pub const CPIO_NEWC_MAGIC: &[u8; 6] = b"070701";
pub const CPIO_CRC_MAGIC: &[u8; 6] = b"070702";

pub fn is_cpio_newc_archive(raw: &[u8]) -> bool {
    raw.len() >= 110 && (&raw[0..6] == CPIO_NEWC_MAGIC || &raw[0..6] == CPIO_CRC_MAGIC)
}

fn parse_cpio_hex8(field: &[u8]) -> Result<u32, String> {
    let s = std::str::from_utf8(field).map_err(|_| "invalid ASCII in CPIO hex header".to_string())?;
    u32::from_str_radix(s, 16).map_err(|_| format!("invalid hex value in CPIO header: '{}'", s))
}

/// Extracts a `RootfsTree` from an SVR4 ASCII `CPIO newc` (`070701` / `070702`) archive.
///
/// ### `swupdate` OTA Container Layout & Encryption Model
/// Official Yandex smart speaker OTA update archives (distributed as `update.zip` or `.swu`
/// for `swupdate`) use an outer SVR4 ASCII `CPIO` container (`070701` or `070702`) where
/// each member has a 110-byte ASCII hexadecimal header (`magic[0..6]`, `mode[14..22]`,
/// `uid[22..30]`, `gid[30..38]`, `filesize[54..62]`, `namesize[94..102]`), followed by a
/// NUL-terminated filename padded to a 4-byte boundary (`(off + 110 + namesize + 3) & !3`)
/// and 4-byte aligned file data (`(data_off + filesize + 3) & !3`), ending at `TRAILER!!!`.
///
/// Inside a typical OTA package (e.g. `yandexmini2`), the CPIO members are:
/// - `sw-description` and `sw-description.sig` (`swupdate` manifest and RSA signature)
/// - `boot.img.encrypt` and `dt.img.encrypt` (Amlogic hardware-encrypted kernel & DTB)
/// - `flash-nand-copies.sh` (flasher helper script)
/// - `rootfs.ubifs` (**unencrypted** UBIFS root filesystem image compressed with `LZO1X-1`)
/// - `u-boot.bin.encrypt.usb.bl2` and `u-boot.bin.encrypt.usb.tpl` (encrypted bootloader stages)
///
/// **Why `rootfs.ubifs` is unencrypted while `boot.img.encrypt` and `dt.img.encrypt` are encrypted**:
/// On Amlogic SoCs (AXG / A1 / A5), the mask ROM (`BL1`) and Secure Monitor enforce hardware
/// Secure Boot encryption on early boot stages (`BL2`, `TPL`, kernel `boot.img`, and device tree
/// `dt.img`) loaded into secure memory before Linux starts. The root filesystem (`rootfs.ubifs`),
/// however, is mounted by the Linux kernel in normal world (`REE`) directly from the raw NAND UBI
/// volume (`ubi0:rootfs`), which does not use block-layer encryption (`dm-crypt`) because UBIFS
/// manages raw flash LEBs directly. Because `kuyase` boots the guest rootfs under its own QEMU
/// kernel, only the unencrypted `rootfs.ubifs` member is needed to construct a bootable `Guest Image`.
pub fn extract_cpio_into_tree(raw: &[u8]) -> Result<RootfsTree, String> {
    if !is_cpio_newc_archive(raw) {
        return Err("not a valid CPIO newc (070701/070702) archive".to_string());
    }

    let mut direct_tree = RootfsTree::default();
    let mut off = 0usize;

    while off + 110 <= raw.len() {
        let magic = &raw[off..off + 6];
        if magic != CPIO_NEWC_MAGIC && magic != CPIO_CRC_MAGIC {
            break;
        }

        let mode = parse_cpio_hex8(&raw[off + 14..off + 22])?;
        let uid = (parse_cpio_hex8(&raw[off + 22..off + 30])? & 0xFFFF) as u16;
        let gid = (parse_cpio_hex8(&raw[off + 30..off + 38])? & 0xFFFF) as u16;
        let filesize = parse_cpio_hex8(&raw[off + 54..off + 62])? as usize;
        let namesize = parse_cpio_hex8(&raw[off + 94..off + 102])? as usize;

        let name_start = off + 110;
        let name_end = name_start
            .checked_add(namesize)
            .ok_or("CPIO namesize overflow")?;
        if name_end > raw.len() {
            return Err("CPIO entry name exceeds archive bounds".to_string());
        }

        let raw_name = String::from_utf8_lossy(&raw[name_start..name_end])
            .trim_end_matches('\0')
            .to_string();

        let data_off = (name_end + 3) & !3;
        let data_end = data_off
            .checked_add(filesize)
            .ok_or("CPIO filesize overflow")?;
        if data_end > raw.len() {
            return Err(format!(
                "CPIO entry '{}' payload ({} bytes) exceeds archive bounds",
                raw_name, filesize
            ));
        }

        let entry_data = &raw[data_off..data_end];
        off = (data_end + 3) & !3;

        if raw_name == "TRAILER!!!" {
            break;
        }

        let lower_name = raw_name.to_ascii_lowercase();
        let base_name = lower_name.rsplit('/').next().unwrap_or(&lower_name);
        let is_named_rootfs_image = base_name == "rootfs.ubifs"
            || base_name.ends_with(".ubifs")
            || base_name == "rootfs.img"
            || base_name == "system.img"
            || base_name == "system.ext4"
            || base_name == "ubi0_0_rootfs.img"
            || base_name == "mtd3_system.bin"
            || base_name.ends_with(".squashfs")
            || base_name == "rootfs.tar.gz"
            || base_name == "rootfs.tgz";

        let has_fs_magic = entry_data.len() >= 24
            && ({
                let m = u32::from_le_bytes([
                    entry_data[0],
                    entry_data[1],
                    entry_data[2],
                    entry_data[3],
                ]);
                m == UBIFS_NODE_MAGIC
                    || m == SQUASHFS_MAGIC
                    || &entry_data[0..4] == UBI_EC_MAGIC
                    || (entry_data.len() >= 2048
                        && u16::from_le_bytes([entry_data[1080], entry_data[1081]])
                            == EXT2_SUPER_MAGIC)
            });

        if is_named_rootfs_image || has_fs_magic {
            if let Ok(inner_tree) = extract_payload_bytes_into_tree(entry_data) {
                if !inner_tree.entries.is_empty() {
                    return Ok(inner_tree);
                }
            }
        }

        // Also record standard POSIX entries in case the CPIO stream itself is a direct rootfs/initramfs
        let fmt = (mode as u16) & 0xF000;
        let perm = (mode as u16) & 0o7777;
        if fmt == ubifs::S_IFDIR {
            direct_tree.insert_dir_with_owner(&raw_name, if perm == 0 { 0o755 } else { perm }, uid, gid);
        } else if fmt == ubifs::S_IFLNK {
            let target = String::from_utf8_lossy(entry_data).to_string();
            direct_tree.insert_symlink_with_owner(&raw_name, target, uid, gid);
        } else if fmt == ubifs::S_IFREG {
            direct_tree.insert_file_with_owner(
                &raw_name,
                entry_data.to_vec(),
                if perm == 0 { 0o644 } else { perm },
                uid,
                gid,
            );
        }
    }

    if !direct_tree.entries.is_empty() {
        return Ok(direct_tree);
    }

    Err("no extractable rootfs image or files found in CPIO archive".to_string())
}

pub(crate) fn decompress_zlib_or_deflate(input: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    if ZlibDecoder::new(input).read_to_end(&mut out).is_ok() && !out.is_empty() {
        return Ok(out);
    }
    out.clear();
    DeflateDecoder::new(input)
        .read_to_end(&mut out)
        .map_err(|e| format!("deflate decompression failed: {}", e))?;
    Ok(out)
}

fn detect_ubi_peb_size(raw: &[u8], first_data_offset: usize) -> usize {
    let min_step = first_data_offset.max(512);
    let mut off = min_step;
    while off + 64 <= raw.len() {
        if &raw[off..off + 4] == UBI_EC_MAGIC && raw[off + 4] == 1 {
            let vid_off =
                u32::from_be_bytes([raw[off + 16], raw[off + 17], raw[off + 18], raw[off + 19]])
                    as usize;
            let data_off =
                u32::from_be_bytes([raw[off + 20], raw[off + 21], raw[off + 22], raw[off + 23]])
                    as usize;
            if vid_off >= 64 && data_off > vid_off && off + data_off <= raw.len() {
                return off;
            }
        }
        off += 64;
    }
    raw.len()
}

pub fn extract_payload_bytes_into_tree(payload: &[u8]) -> Result<RootfsTree, String> {
    if payload.starts_with(&[0x1f, 0x8b]) {
        let mut dec = Vec::new();
        if GzDecoder::new(Cursor::new(payload))
            .read_to_end(&mut dec)
            .is_ok()
            && !dec.is_empty()
        {
            return extract_payload_bytes_into_tree(&dec);
        }
    }
    if is_cpio_newc_archive(payload) {
        return extract_cpio_into_tree(payload);
    }
    if payload.len() >= 4 && &payload[0..4] == UBI_EC_MAGIC {
        return extract_ubi_into_tree(payload);
    }
    if payload.len() >= 4 {
        let magic_le = u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]);
        if magic_le == SQUASHFS_MAGIC {
            return extract_squashfs_into_tree(payload);
        }
        if magic_le == UBIFS_NODE_MAGIC {
            return extract_ubifs_into_tree(payload);
        }
    }
    if payload.len() >= 2048
        && u16::from_le_bytes([payload[1080], payload[1081]]) == EXT2_SUPER_MAGIC
    {
        let tmp = std::env::temp_dir().join(format!(
            "kuyase-ubi-ext-{}-{}.img",
            std::process::id(),
            payload.len()
        ));
        fs::write(&tmp, payload).map_err(|e| e.to_string())?;
        let res = read_ext2_image(&tmp);
        let _ = fs::remove_file(&tmp);
        return res;
    }
    if payload.windows(4).any(|w| w == [0x31, 0x18, 0x10, 0x06]) {
        if let Ok(tree) = extract_ubifs_into_tree(payload) {
            return Ok(tree);
        }
    }
    if let Some(pos) = payload.windows(4).position(|w| w == b"hsqs") {
        if let Ok(tree) = extract_squashfs_into_tree(&payload[pos..]) {
            return Ok(tree);
        }
    }
    if let Ok(t) = read_tar_from_reader(Cursor::new(payload)) {
        if !t.entries.is_empty() {
            return Ok(t);
        }
    }
    Err("unrecognized volume payload inside UBI image".to_string())
}

pub fn extract_ubi_into_tree(raw: &[u8]) -> Result<RootfsTree, String> {
    if raw.len() < 128 || &raw[0..4] != UBI_EC_MAGIC {
        return Err("not a valid UBI# image".to_string());
    }
    let first_data_off = u32::from_be_bytes([raw[20], raw[21], raw[22], raw[23]]) as usize;
    let peb_size = detect_ubi_peb_size(raw, first_data_off);
    if peb_size < 128 {
        return Err("invalid UBI PEB size".to_string());
    }

    // Map (vol_id, lnum) -> (sqnum, slice)
    let mut volumes: BTreeMap<u32, BTreeMap<u32, (u64, &[u8])>> = BTreeMap::new();
    let mut peb_off = 0usize;
    while peb_off + 64 <= raw.len() {
        let peb = &raw[peb_off..(peb_off + peb_size).min(raw.len())];
        if &peb[0..4] == UBI_EC_MAGIC {
            let vid_off = u32::from_be_bytes([peb[16], peb[17], peb[18], peb[19]]) as usize;
            let data_off = u32::from_be_bytes([peb[20], peb[21], peb[22], peb[23]]) as usize;
            if vid_off + 64 <= peb.len() && data_off <= peb.len() {
                let vid = &peb[vid_off..vid_off + 64];
                if &vid[0..4] == UBI_VID_MAGIC {
                    let vol_type = vid[5];
                    let vol_id = u32::from_be_bytes([vid[8], vid[9], vid[10], vid[11]]);
                    let lnum = u32::from_be_bytes([vid[12], vid[13], vid[14], vid[15]]);
                    let data_size =
                        u32::from_be_bytes([vid[20], vid[21], vid[22], vid[23]]) as usize;
                    let sqnum = u64::from_be_bytes([
                        vid[36], vid[37], vid[38], vid[39], vid[40], vid[41], vid[42], vid[43],
                    ]);

                    if vol_id < 0x7FFF_EFFF {
                        let leb_cap = peb.len() - data_off;
                        let take = if vol_type == 2 && data_size > 0 && data_size <= leb_cap {
                            data_size
                        } else {
                            leb_cap
                        };
                        let data_slice = &peb[data_off..data_off + take];
                        let vol_map = volumes.entry(vol_id).or_default();
                        let replace = vol_map
                            .get(&lnum)
                            .map(|(old_sq, _)| sqnum >= *old_sq)
                            .unwrap_or(true);
                        if replace {
                            vol_map.insert(lnum, (sqnum, data_slice));
                        }
                    }
                }
            }
        }
        peb_off += peb_size;
    }

    for (_vol_id, lebs) in volumes {
        let mut reassembled = Vec::new();
        for (_lnum, (_sq, bytes)) in lebs {
            reassembled.extend_from_slice(bytes);
        }
        if let Ok(tree) = extract_payload_bytes_into_tree(&reassembled) {
            if !tree.entries.is_empty() {
                return Ok(tree);
            }
        }
    }

    // Fallback: scan raw image directly
    extract_payload_bytes_into_tree(raw)
}

#[cfg(test)]
pub(crate) fn append_test_cpio_entry(
    out: &mut Vec<u8>,
    magic: &[u8; 6],
    ino: u32,
    mode: u32,
    name: &str,
    data: &[u8],
) {
    let namesize = name.len() + 1;
    let header = format!(
        "{}{:08X}{:08X}{:08X}{:08X}{:08X}{:08X}{:08X}{:08X}{:08X}{:08X}{:08X}{:08X}{:08X}",
        std::str::from_utf8(magic).unwrap(),
        ino,
        mode,
        0u32, // uid
        0u32, // gid
        1u32, // nlink
        0u32, // mtime
        data.len(),
        0u32, // devmajor
        0u32, // devminor
        0u32, // rdevmajor
        0u32, // rdevminor
        namesize,
        0u32 // check
    );
    assert_eq!(header.len(), 110);
    out.extend_from_slice(header.as_bytes());
    out.extend_from_slice(name.as_bytes());
    out.push(0);
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
    out.extend_from_slice(data);
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
}

#[cfg(test)]
pub(crate) fn build_test_ubifs_ino_node(
    sqnum: u64,
    inum: u32,
    mode: u16,
    uid: u16,
    gid: u16,
    size: u64,
    inline_data: &[u8],
) -> Vec<u8> {
    let len = 160 + inline_data.len();
    let mut buf = vec![0u8; (len + 7) & !7];
    buf[0..4].copy_from_slice(&UBIFS_NODE_MAGIC.to_le_bytes());
    buf[8..16].copy_from_slice(&sqnum.to_le_bytes());
    buf[16..20].copy_from_slice(&(len as u32).to_le_bytes());
    buf[20] = ubifs::UBIFS_INO_NODE;
    buf[24..28].copy_from_slice(&inum.to_le_bytes());
    buf[48..56].copy_from_slice(&size.to_le_bytes());
    buf[92..96].copy_from_slice(&1u32.to_le_bytes()); // nlink = 1
    buf[96..100].copy_from_slice(&(uid as u32).to_le_bytes());
    buf[100..104].copy_from_slice(&(gid as u32).to_le_bytes());
    buf[104..108].copy_from_slice(&(mode as u32).to_le_bytes());
    buf[112..116].copy_from_slice(&(inline_data.len() as u32).to_le_bytes());
    buf[160..160 + inline_data.len()].copy_from_slice(inline_data);
    buf
}

#[cfg(test)]
pub(crate) fn build_test_ubifs_dent_node(
    sqnum: u64,
    parent_inum: u32,
    target_inum: u32,
    entry_type: u8,
    name: &str,
) -> Vec<u8> {
    let nb = name.as_bytes();
    let len = 56 + nb.len() + 1;
    let mut buf = vec![0u8; (len + 7) & !7];
    buf[0..4].copy_from_slice(&UBIFS_NODE_MAGIC.to_le_bytes());
    buf[8..16].copy_from_slice(&sqnum.to_le_bytes());
    buf[16..20].copy_from_slice(&(len as u32).to_le_bytes());
    buf[20] = ubifs::UBIFS_DENT_NODE;
    buf[24..28].copy_from_slice(&parent_inum.to_le_bytes());
    buf[40..48].copy_from_slice(&(target_inum as u64).to_le_bytes());
    buf[49] = entry_type;
    buf[50..52].copy_from_slice(&(nb.len() as u16).to_le_bytes());
    buf[56..56 + nb.len()].copy_from_slice(nb);
    buf
}

#[cfg(test)]
pub(crate) fn build_test_ubifs_compressed_data_node(
    sqnum: u64,
    inum: u32,
    block_num: u32,
    uncomp_len: usize,
    compr_type: u16,
    compressed_payload: &[u8],
) -> Vec<u8> {
    let len = 48 + compressed_payload.len();
    let mut buf = vec![0u8; (len + 7) & !7];
    buf[0..4].copy_from_slice(&UBIFS_NODE_MAGIC.to_le_bytes());
    buf[8..16].copy_from_slice(&sqnum.to_le_bytes());
    buf[16..20].copy_from_slice(&(len as u32).to_le_bytes());
    buf[20] = ubifs::UBIFS_DATA_NODE;
    buf[24..28].copy_from_slice(&inum.to_le_bytes());
    let k1 = (1u32 << 29) | (block_num & 0x1FFF_FFFF);
    buf[28..32].copy_from_slice(&k1.to_le_bytes());
    buf[40..44].copy_from_slice(&(uncomp_len as u32).to_le_bytes());
    buf[44..46].copy_from_slice(&compr_type.to_le_bytes());
    buf[48..48 + compressed_payload.len()].copy_from_slice(compressed_payload);
    buf
}

#[cfg(test)]
mod tests {
    use super::ubifs::{
        decompress_lzo1x, S_IFDIR, S_IFLNK, S_IFREG, UBIFS_COMPR_LZO, UBIFS_COMPR_NONE,
    };
    use super::{
        build_test_ubifs_compressed_data_node as build_ubifs_compressed_data_node,
        build_test_ubifs_dent_node as build_ubifs_dent_node,
        build_test_ubifs_ino_node as build_ubifs_ino_node, *,
    };
    use crate::image_builder::RootfsNodeKind;

    fn build_ubi_peb(
        peb_size: usize,
        vol_id: u32,
        lnum: u32,
        sqnum: u64,
        leb_payload: &[u8],
    ) -> Vec<u8> {
        let vid_off = 64u32;
        let data_off = 128u32;
        let mut peb = vec![0xFFu8; peb_size];
        // EC header
        peb[0..4].copy_from_slice(UBI_EC_MAGIC);
        peb[4] = 1;
        peb[16..20].copy_from_slice(&vid_off.to_be_bytes());
        peb[20..24].copy_from_slice(&data_off.to_be_bytes());
        // VID header
        let vid = &mut peb[vid_off as usize..vid_off as usize + 64];
        vid.fill(0);
        vid[0..4].copy_from_slice(UBI_VID_MAGIC);
        vid[4] = 1;
        vid[5] = 1; // dynamic volume
        vid[8..12].copy_from_slice(&vol_id.to_be_bytes());
        vid[12..16].copy_from_slice(&lnum.to_be_bytes());
        vid[20..24].copy_from_slice(&(leb_payload.len() as u32).to_be_bytes());
        vid[36..44].copy_from_slice(&sqnum.to_be_bytes());
        // LEB data
        peb[data_off as usize..data_off as usize + leb_payload.len()].copy_from_slice(leb_payload);
        peb
    }

    fn build_ubifs_data_node(sqnum: u64, inum: u32, block_num: u32, data: &[u8]) -> Vec<u8> {
        build_ubifs_compressed_data_node(sqnum, inum, block_num, data.len(), UBIFS_COMPR_NONE, data)
    }

    #[test]
    fn test_extract_synthetic_ubi_ubifs_image() {
        let mut leb0 = Vec::new();
        // Root dir (inum 1), /bin dir (inum 2), /bin/busybox file (inum 3), /bin/sh symlink (inum 4)
        leb0.extend_from_slice(&build_ubifs_ino_node(1, 1, S_IFDIR | 0o755, 0, 0, 0, b""));
        leb0.extend_from_slice(&build_ubifs_dent_node(2, 1, 2, 1, "bin"));
        leb0.extend_from_slice(&build_ubifs_ino_node(3, 2, S_IFDIR | 0o755, 0, 0, 0, b""));

        let mut leb1 = Vec::new();
        let busybox_bytes = b"\x7fELF-ubifs-busybox";
        leb1.extend_from_slice(&build_ubifs_dent_node(4, 2, 3, 0, "busybox"));
        leb1.extend_from_slice(&build_ubifs_ino_node(
            5,
            3,
            S_IFREG | 0o755,
            1000,
            1000,
            busybox_bytes.len() as u64,
            b"",
        ));
        leb1.extend_from_slice(&build_ubifs_data_node(6, 3, 0, busybox_bytes));
        leb1.extend_from_slice(&build_ubifs_dent_node(7, 2, 4, 2, "sh"));
        leb1.extend_from_slice(&build_ubifs_ino_node(
            8,
            4,
            S_IFLNK | 0o777,
            0,
            0,
            7,
            b"busybox",
        ));

        // Put LEB 1 in PEB 0 and LEB 0 in PEB 1 out-of-order to verify UBI (vol_id, lnum) sorting!
        let mut ubi_img = Vec::new();
        ubi_img.extend_from_slice(&build_ubi_peb(2048, 0, 1, 20, &leb1));
        ubi_img.extend_from_slice(&build_ubi_peb(2048, 0, 0, 10, &leb0));

        let tree = extract_ubi_into_tree(&ubi_img).unwrap();
        let bb = tree.get("bin/busybox").unwrap();
        assert_eq!(bb.mode, 0o755);
        assert_eq!(bb.uid, 1000);
        assert_eq!(bb.gid, 1000);
        assert_eq!(tree.get_file_bytes("bin/busybox").unwrap(), busybox_bytes);
        assert_eq!(
            tree.get("bin/sh").unwrap().kind,
            RootfsNodeKind::Symlink("busybox".to_string())
        );
    }

    #[test]
    fn test_extract_synthetic_ubi_squashfs_image() {
        // Build a valid uncompressed SquashFS 4.0 image with root dir (ino 1) -> file "hello.txt" (ino 2)
        let file_content = b"squashfs-inside-ubi-payload";
        let mut sqsh = vec![0u8; 512];

        // Data block for hello.txt at offset 96
        let data_start = 96usize;
        sqsh[data_start..data_start + file_content.len()].copy_from_slice(file_content);

        // Inode table metadata block at offset 160 (uncompressed flag 0x8000)
        // Inode 2 (basic file, 36 bytes) at inner_off 0; Inode 1 (basic dir, 32 bytes) at inner_off 36
        let inode_table_start = 160u64;
        let mut ino_meta = Vec::new();
        // Inode 2: basic file (type = 2)
        ino_meta.extend_from_slice(&2u16.to_le_bytes()); // inode_type = 2
        ino_meta.extend_from_slice(&0o644u16.to_le_bytes()); // mode
        ino_meta.extend_from_slice(&0u16.to_le_bytes()); // uid_idx = 0
        ino_meta.extend_from_slice(&0u16.to_le_bytes()); // gid_idx = 0
        ino_meta.extend_from_slice(&0u32.to_le_bytes()); // mtime
        ino_meta.extend_from_slice(&2u32.to_le_bytes()); // ino = 2
        ino_meta.extend_from_slice(&(data_start as u32).to_le_bytes()); // blocks_start
        ino_meta.extend_from_slice(&u32::MAX.to_le_bytes()); // fragment_block_index = NONE
        ino_meta.extend_from_slice(&0u32.to_le_bytes()); // block_offset
        ino_meta.extend_from_slice(&(file_content.len() as u32).to_le_bytes()); // file_size
        let blk_sz_hdr = (file_content.len() as u32) | 0x0100_0000; // uncompressed block
        ino_meta.extend_from_slice(&blk_sz_hdr.to_le_bytes());
        assert_eq!(ino_meta.len(), 36);

        // Directory table entry for "hello.txt" -> 12-byte header + 8-byte entry + 9-byte name = 29 bytes
        let mut dir_meta = Vec::new();
        dir_meta.extend_from_slice(&0u32.to_le_bytes()); // count - 1 = 0 (1 entry)
        dir_meta.extend_from_slice(&0u32.to_le_bytes()); // start_block = 0
        dir_meta.extend_from_slice(&1u32.to_le_bytes()); // inode_number = 1
        dir_meta.extend_from_slice(&0u16.to_le_bytes()); // entry_offset = 0 (points to Inode 2)
        dir_meta.extend_from_slice(&1i16.to_le_bytes()); // inode_offset = +1
        dir_meta.extend_from_slice(&2u16.to_le_bytes()); // type = file
        dir_meta.extend_from_slice(&8u16.to_le_bytes()); // name_size - 1 = 8 ("hello.txt" len 9)
        dir_meta.extend_from_slice(b"hello.txt");

        // Inode 1: basic dir (type = 1) at inner_off 36
        let root_inner_off = ino_meta.len() as u64;
        ino_meta.extend_from_slice(&1u16.to_le_bytes()); // inode_type = 1
        ino_meta.extend_from_slice(&0o755u16.to_le_bytes()); // mode
        ino_meta.extend_from_slice(&0u16.to_le_bytes());
        ino_meta.extend_from_slice(&0u16.to_le_bytes());
        ino_meta.extend_from_slice(&0u32.to_le_bytes());
        ino_meta.extend_from_slice(&1u32.to_le_bytes()); // ino = 1
        ino_meta.extend_from_slice(&0u32.to_le_bytes()); // dir_block_start = 0
        ino_meta.extend_from_slice(&2u32.to_le_bytes()); // nlink = 2
        ino_meta.extend_from_slice(&((dir_meta.len() + 3) as u16).to_le_bytes()); // file_size
        ino_meta.extend_from_slice(&0u16.to_le_bytes()); // offset = 0
        ino_meta.extend_from_slice(&1u32.to_le_bytes()); // parent_inode = 1

        let ino_hdr = (ino_meta.len() as u16) | 0x8000;
        sqsh[inode_table_start as usize..inode_table_start as usize + 2]
            .copy_from_slice(&ino_hdr.to_le_bytes());
        sqsh[inode_table_start as usize + 2..inode_table_start as usize + 2 + ino_meta.len()]
            .copy_from_slice(&ino_meta);

        let directory_table_start = (inode_table_start as usize + 2 + ino_meta.len()) as u64;
        let dir_hdr = (dir_meta.len() as u16) | 0x8000;
        sqsh[directory_table_start as usize..directory_table_start as usize + 2]
            .copy_from_slice(&dir_hdr.to_le_bytes());
        sqsh[directory_table_start as usize + 2
            ..directory_table_start as usize + 2 + dir_meta.len()]
            .copy_from_slice(&dir_meta);

        // Superblock at 0..96
        sqsh[0..4].copy_from_slice(&SQUASHFS_MAGIC.to_le_bytes());
        sqsh[4..8].copy_from_slice(&2u32.to_le_bytes()); // inodes = 2
        sqsh[12..16].copy_from_slice(&4096u32.to_le_bytes()); // block_size = 4096
        sqsh[20..22].copy_from_slice(&1u16.to_le_bytes()); // compression = 1 (gzip)
        sqsh[22..24].copy_from_slice(&12u16.to_le_bytes()); // block_log = 12
        sqsh[28..30].copy_from_slice(&4u16.to_le_bytes()); // major = 4
        sqsh[30..32].copy_from_slice(&0u16.to_le_bytes()); // minor = 0
        sqsh[32..40].copy_from_slice(&root_inner_off.to_le_bytes()); // root_inode
        sqsh[48..56].copy_from_slice(&u64::MAX.to_le_bytes()); // id_table_start
        sqsh[64..72].copy_from_slice(&inode_table_start.to_le_bytes());
        sqsh[72..80].copy_from_slice(&directory_table_start.to_le_bytes());
        sqsh[80..88].copy_from_slice(&u64::MAX.to_le_bytes()); // fragment_table_start

        let ubi_sqsh = build_ubi_peb(1024, 0, 0, 1, &sqsh);
        let tree = extract_ubi_into_tree(&ubi_sqsh).unwrap();
        assert_eq!(tree.get_file_bytes("hello.txt").unwrap(), file_content);
        assert_eq!(tree.get("hello.txt").unwrap().mode, 0o644);
    }

    fn build_ubifs_pad_node(sqnum: u64, pad_len: u32) -> Vec<u8> {
        let len = 28usize;
        let mut buf = vec![0u8; (len + 7) & !7];
        buf[0..4].copy_from_slice(&UBIFS_NODE_MAGIC.to_le_bytes());
        buf[8..16].copy_from_slice(&sqnum.to_le_bytes());
        buf[16..20].copy_from_slice(&(len as u32).to_le_bytes());
        buf[20] = 5; // UBIFS_PAD_NODE
        buf[24..28].copy_from_slice(&pad_len.to_le_bytes());
        buf
    }

    #[test]
    fn test_lzo1x_decompressor_literals_m1_m2_m3_m4_and_overlapping_rle() {
        // 1. Short initial literal (t = 3 < 4 -> direct Match -> EOF)
        let short_stream = [17 + 3, b'A', b'B', b'C', 0x11, 0x00, 0x00];
        let dec_short = decompress_lzo1x(&short_stream, 3).unwrap();
        assert_eq!(dec_short, b"ABC");

        // 2. Initial literal (t = 6 >= 4) + M2 match (len = 6, dist = 6) + 2 trailing literals + M1 match (len = 2, dist = 2) + EOF
        // "quasar" (6 B) -> M2 copies "quasar" (dist 6, len 6) + trailing "!!" -> M1 copies "!!" (dist 2, len 2)
        // M2: m_len = 6 -> (6 - 1) << 5 = 5 << 5 = 0xA0; d = 6 - 1 = 5 -> (5 & 7) << 2 = 0x14; trailing = 2 -> b0 = 0xB6, b1 = 0x00
        // M1: t < 16, dist = 2 -> d = 1 -> (t >> 2) = 1, b1 = 0, trailing = 0 -> t = 0x04, b1 = 0x00
        let mut m2_m1_stream = Vec::new();
        m2_m1_stream.push(17 + 6);
        m2_m1_stream.extend_from_slice(b"quasar");
        m2_m1_stream.extend_from_slice(&[0xB6, 0x00]); // M2 len=6, dist=6, trailing=2
        m2_m1_stream.extend_from_slice(b"!!"); // 2 trailing literals
        m2_m1_stream.extend_from_slice(&[0x04, 0x00]); // M1 len=2, dist=2, trailing=0
        m2_m1_stream.extend_from_slice(&[0x11, 0x00, 0x00]); // EOF
        let dec_m2_m1 = decompress_lzo1x(&m2_m1_stream, 16).unwrap();
        assert_eq!(dec_m2_m1, b"quasarquasar!!!!");

        // 3. Long zero-extended literal run in OuterLoop (300 bytes) + overlapping M3 RLE match (len = 50, dist = 1)
        let mut long_stream = Vec::new();
        // t == 0 in OuterLoop: 300 = 3 + 15 + 255 + 27 -> [0x00, 0x00, 27]
        long_stream.extend_from_slice(&[0x00, 0x00, 27]);
        let mut expected = Vec::new();
        for i in 0..300usize {
            let b = b'a' + ((i % 26) as u8);
            long_stream.push(b);
            expected.push(b);
        }
        // Now in FirstLiteralRun: emit M3 match (32 <= t < 64) with zero-extended length 50 (m_len - 2 = 48 = 31 + 17)
        // t = 0x20 (32, m_len & 31 == 0), next byte = 17 -> m_len = 31 + 17 + 2 = 50
        // dist = 1 -> d = 0 -> v = (0 << 2) | 0 = 0x0000
        long_stream.extend_from_slice(&[0x20, 17, 0x00, 0x00]);
        let last_byte = *expected.last().unwrap();
        expected.resize(expected.len() + 50, last_byte);
        long_stream.extend_from_slice(&[0x11, 0x00, 0x00]); // EOF

        let dec_long = decompress_lzo1x(&long_stream, expected.len()).unwrap();
        assert_eq!(dec_long, expected);
    }

    #[test]
    fn test_extract_cpio_newc_and_crc_with_ubifs_lzo1x_and_padded_lebs() {
        // Build a UBIFS image containing:
        // - /system/vendor/quasar/quasar.cfg (compressed with UBIFS_COMPR_LZO = 1)
        // - /bin/busybox (compressed with UBIFS_COMPR_LZO = 1 across a padded LEB gap)
        let cfg_plain = br#"{"deviceType": "yandexmini_2"}"#;
        // Encode cfg_plain with LZO1X-1: initial literal of 16 bytes `{"deviceType": "` +
        // OuterLoop literal or full initial literal + M2 match
        let mut cfg_lzo = Vec::new();
        cfg_lzo.push(17 + (cfg_plain.len() as u8));
        cfg_lzo.extend_from_slice(cfg_plain);
        cfg_lzo.extend_from_slice(&[0x11, 0x00, 0x00]);

        // Encode busybox with LZO1X-1 using an M2 repetition: "\x7fELF-lzo-busybox-\x7fELF-lzo"
        let bb_prefix = b"\x7fELF-lzo-busybox-";
        let bb_plain = b"\x7fELF-lzo-busybox-\x7fELF-lzo";
        let mut bb_lzo = Vec::new();
        bb_lzo.push(17 + (bb_prefix.len() as u8));
        bb_lzo.extend_from_slice(bb_prefix);
        // M2 match: len = 8 ((8 - 1) << 5 = 0xE0), dist = 17 (d = 16 -> (16 & 7) << 2 = 0, b1 = 16 >> 3 = 2)
        bb_lzo.extend_from_slice(&[0xE0, 0x02]);
        bb_lzo.extend_from_slice(&[0x11, 0x00, 0x00]);
        assert_eq!(decompress_lzo1x(&bb_lzo, bb_plain.len()).unwrap(), bb_plain);

        let mut ubifs_img = Vec::new();
        // LEB 0: root (1) -> system (2) -> vendor (3) -> quasar (4) -> quasar.cfg (5)
        ubifs_img.extend_from_slice(&build_ubifs_ino_node(1, 1, S_IFDIR | 0o755, 0, 0, 0, b""));
        ubifs_img.extend_from_slice(&build_ubifs_dent_node(2, 1, 2, 1, "system"));
        ubifs_img.extend_from_slice(&build_ubifs_ino_node(3, 2, S_IFDIR | 0o755, 0, 0, 0, b""));
        ubifs_img.extend_from_slice(&build_ubifs_dent_node(4, 2, 3, 1, "vendor"));
        ubifs_img.extend_from_slice(&build_ubifs_ino_node(5, 3, S_IFDIR | 0o755, 0, 0, 0, b""));
        ubifs_img.extend_from_slice(&build_ubifs_dent_node(6, 3, 4, 1, "quasar"));
        ubifs_img.extend_from_slice(&build_ubifs_ino_node(7, 4, S_IFDIR | 0o755, 0, 0, 0, b""));
        ubifs_img.extend_from_slice(&build_ubifs_dent_node(8, 4, 5, 0, "quasar.cfg"));
        ubifs_img.extend_from_slice(&build_ubifs_ino_node(
            9,
            5,
            S_IFREG | 0o644,
            0,
            0,
            cfg_plain.len() as u64,
            b"",
        ));
        ubifs_img.extend_from_slice(&build_ubifs_compressed_data_node(
            10,
            5,
            0,
            cfg_plain.len(),
            UBIFS_COMPR_LZO,
            &cfg_lzo,
        ));

        // Add UBIFS_PAD_NODE + 8192 bytes of 0xFF LEB padding + extra 4096 bytes of 0xFF tail
        ubifs_img.extend_from_slice(&build_ubifs_pad_node(11, 8192));
        ubifs_img.extend(std::iter::repeat_n(0xFFu8, 8192 + 4096));

        // LEB 1: root (1) -> bin (6) -> busybox (7)
        ubifs_img.extend_from_slice(&build_ubifs_dent_node(12, 1, 6, 1, "bin"));
        ubifs_img.extend_from_slice(&build_ubifs_ino_node(13, 6, S_IFDIR | 0o755, 0, 0, 0, b""));
        ubifs_img.extend_from_slice(&build_ubifs_dent_node(14, 6, 7, 0, "busybox"));
        ubifs_img.extend_from_slice(&build_ubifs_ino_node(
            15,
            7,
            S_IFREG | 0o755,
            0,
            0,
            bb_plain.len() as u64,
            b"",
        ));
        ubifs_img.extend_from_slice(&build_ubifs_compressed_data_node(
            16,
            7,
            0,
            bb_plain.len(),
            UBIFS_COMPR_LZO,
            &bb_lzo,
        ));

        for magic in [CPIO_NEWC_MAGIC, CPIO_CRC_MAGIC] {
            let mut cpio_ota = Vec::new();
            append_test_cpio_entry(
                &mut cpio_ota,
                magic,
                1,
                0o100644,
                "sw-description",
                b"software = { version = \"0.367\"; };",
            );
            append_test_cpio_entry(
                &mut cpio_ota,
                magic,
                2,
                0o100644,
                "boot.img.encrypt",
                b"AML-ENCRYPTED-KERNEL-STUB",
            );
            append_test_cpio_entry(
                &mut cpio_ota,
                magic,
                3,
                0o100644,
                "dt.img.encrypt",
                b"AML-ENCRYPTED-DTB-STUB",
            );
            append_test_cpio_entry(
                &mut cpio_ota,
                magic,
                4,
                0o100644,
                "rootfs.ubifs",
                &ubifs_img,
            );
            append_test_cpio_entry(&mut cpio_ota, magic, 5, 0, "TRAILER!!!", b"");

            assert!(is_cpio_newc_archive(&cpio_ota));
            let tree = extract_payload_bytes_into_tree(&cpio_ota).unwrap();
            assert_eq!(
                tree.get_file_bytes("system/vendor/quasar/quasar.cfg")
                    .unwrap(),
                cfg_plain
            );
            assert_eq!(tree.get_file_bytes("bin/busybox").unwrap(), bb_plain);
            assert_eq!(tree.get("bin/busybox").unwrap().mode, 0o755);
        }
    }
}
