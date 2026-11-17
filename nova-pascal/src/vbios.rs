//! Clean-room NVIDIA PCI Option ROM & BIT (`BIOS Information Table`) parser.

use crate::{NvError, Result};

const ROM_SIGNATURE: u16 = 0xaa55;
const PCIR_MAGIC: &[u8; 4] = b"PCIR";
const BIT_SIGNATURE: &[u8; 6] = b"\xff\xb8BIT\0";

/// Known `PCIR` image `code_type` values in NVIDIA Option ROM images.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RomCodeType {
    /// Legacy x86 / BIOS Data / BIT image (`0x00`, carries Pascal DEVINIT + BIT).
    PciAtX86,
    /// EFI GOP image (`0x03`).
    Efi,
    /// NVIDIA Firmware Security / PMU image (`0xe0`, Turing+ `FWSEC`).
    NvidiaFwsec,
    /// Any other code type value.
    Other(u8),
}

impl RomCodeType {
    pub const fn from_u8(val: u8) -> Self {
        match val {
            0x00 => Self::PciAtX86,
            0x03 => Self::Efi,
            0xe0 => Self::NvidiaFwsec,
            other => Self::Other(other),
        }
    }
}

/// Summary of an individual PCI expansion ROM image inside VBIOS.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct PcirImage {
    pub offset: usize,
    pub length: usize,
    pub vendor_id: u16,
    pub device_id: u16,
    pub code_type: RomCodeType,
    pub is_last: bool,
}

/// Decoded `BIT` token descriptor inside the primary `0x00` ROM image.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct BitToken {
    pub id: u8,
    pub data_version: u8,
    pub data_size: u16,
    pub data_offset: u16,
}

/// Top-level VBIOS summary without heap allocations (`no_std`).
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct VbiosSummary {
    pub image_count: usize,
    pub has_efi_image: bool,
    pub has_fwsec_image: bool,
    /// Total byte span of EFI GOP (`0x03`) image(s) skipped by BIT `0x70` pointers (`0x10a00` on Tesla P4).
    pub efi_gap_bytes: usize,
    pub bit_offset: usize,
    pub bit_token_count: u8,
    /// Non-zero when `BIT` token `0x49` (`DEVINIT`) is present.
    pub devinit_token: Option<BitToken>,
    /// Non-zero when `BIT` token `0x70` (`FALCON_DATA`) is present.
    pub falcon_data_token: Option<BitToken>,
}

/// Decoded entry from `BIT` token `0x70` (`FALCON_DATA`) Ucode Table.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct VbiosFalconAppEntry {
    pub app_id: u8,
    pub target_id: u8,
    /// Actual byte offset inside the full Option ROM image (after `+efi_gap_bytes` adjustment).
    pub adjusted_rom_offset: usize,
    /// Lower 16-bit header version/tag at `adjusted_rom_offset` (e.g. `0x0201` on Devinit v2/HULK).
    pub header_tag: u16,
}

/// Decoded `JFFS` InfoROM directory summary from raw `NV_PROM` (`0x300000`).
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct PromInforomSummary {
    pub jffs_offset: usize,
    pub version: u32,
    pub fs_size: u32,
    pub object_count: usize,
}

fn read_u16(buf: &[u8], off: usize) -> Result<u16> {
    let bytes: [u8; 2] = buf
        .get(off..off + 2)
        .ok_or(NvError::OutOfBounds)?
        .try_into()
        .map_err(|_| NvError::OutOfBounds)?;
    Ok(u16::from_le_bytes(bytes))
}

fn read_u32(buf: &[u8], off: usize) -> Result<u32> {
    let bytes: [u8; 4] = buf
        .get(off..off + 4)
        .ok_or(NvError::OutOfBounds)?
        .try_into()
        .map_err(|_| NvError::OutOfBounds)?;
    Ok(u32::from_le_bytes(bytes))
}

fn parse_pcir_at(rom: &[u8], img_off: usize) -> Result<PcirImage> {
    if read_u16(rom, img_off)? != ROM_SIGNATURE {
        return Err(NvError::InvalidRomHeader);
    }
    let pcir_rel = read_u16(rom, img_off + 0x18)? as usize;
    let pcir_off = img_off + pcir_rel;
    let magic = rom.get(pcir_off..pcir_off + 4).ok_or(NvError::InvalidRomHeader)?;
    if magic != PCIR_MAGIC {
        return Err(NvError::InvalidRomHeader);
    }
    let vendor_id = read_u16(rom, pcir_off + 0x04)?;
    let device_id = read_u16(rom, pcir_off + 0x06)?;
    let image_blocks = read_u16(rom, pcir_off + 0x10)? as usize;
    let code_type = *rom.get(pcir_off + 0x14).ok_or(NvError::OutOfBounds)?;
    let indicator = *rom.get(pcir_off + 0x15).ok_or(NvError::OutOfBounds)?;
    let length = image_blocks.saturating_mul(512);
    if length == 0 || img_off + length > rom.len() {
        return Err(NvError::InvalidRomHeader);
    }
    Ok(PcirImage {
        offset: img_off,
        length,
        vendor_id,
        device_id,
        code_type: RomCodeType::from_u8(code_type),
        is_last: (indicator & 0x80) != 0,
    })
}

fn find_bit_header(image: &[u8]) -> Option<usize> {
    image
        .windows(BIT_SIGNATURE.len())
        .position(|w| w == BIT_SIGNATURE)
}

/// Inspect a complete VBIOS ROM buffer (extracted from PROM/PRAMIN or file) and
/// decode expansion images + `BIT` tokens.
///
/// Tesla P4 (`GP104`) commonly exposes no `/sys/bus/pci/devices/.../rom` sysfs file;
/// callers supply PRAMIN (`0x700000`) or PROM (`0x300000`) bytes when probing on host.
pub fn parse_vbios(rom: &[u8]) -> Result<VbiosSummary> {
    let mut offset = 0usize;
    let mut image_count = 0usize;
    let mut has_efi_image = false;
    let mut has_fwsec_image = false;
    let mut efi_gap_bytes = 0usize;
    let mut primary_range: Option<(usize, usize)> = None;

    while offset < rom.len() && image_count < 16 {
        let img = parse_pcir_at(rom, offset)?;
        image_count += 1;
        match img.code_type {
            RomCodeType::PciAtX86 => {
                if primary_range.is_none() {
                    primary_range = Some((img.offset, img.offset + img.length));
                }
            }
            RomCodeType::Efi => {
                has_efi_image = true;
                efi_gap_bytes = efi_gap_bytes.saturating_add(img.length);
            }
            RomCodeType::NvidiaFwsec => has_fwsec_image = true,
            RomCodeType::Other(_) => {}
        }
        offset += img.length;
        if img.is_last {
            break;
        }
    }

    let (p_start, p_end) = primary_range.ok_or(NvError::InvalidRomHeader)?;
    let primary_slice = &rom[p_start..p_end];
    let rel_bit = find_bit_header(primary_slice).ok_or(NvError::InvalidRomHeader)?;
    let bit_off = p_start + rel_bit;

    let hdr_size = *rom.get(bit_off + 8).ok_or(NvError::OutOfBounds)? as usize;
    let tok_size = *rom.get(bit_off + 9).ok_or(NvError::OutOfBounds)? as usize;
    let tok_count = *rom.get(bit_off + 10).ok_or(NvError::OutOfBounds)?;
    if hdr_size < 12 || tok_size < 6 {
        return Err(NvError::InvalidRomHeader);
    }

    let mut devinit_token = None;
    let mut falcon_data_token = None;
    for i in 0..(tok_count as usize) {
        let t_off = bit_off + hdr_size + i * tok_size;
        let tok = BitToken {
            id: *rom.get(t_off).ok_or(NvError::OutOfBounds)?,
            data_version: *rom.get(t_off + 1).ok_or(NvError::OutOfBounds)?,
            data_size: read_u16(rom, t_off + 2)?,
            data_offset: read_u16(rom, t_off + 4)?,
        };
        if tok.id == 0x49 {
            devinit_token = Some(tok);
        } else if tok.id == 0x70 {
            falcon_data_token = Some(tok);
        }
    }

    Ok(VbiosSummary {
        image_count,
        has_efi_image,
        has_fwsec_image,
        efi_gap_bytes,
        bit_offset: bit_off,
        bit_token_count: tok_count,
        devinit_token,
        falcon_data_token,
    })
}

/// Adjust a `BIT 'p'` / Falcon table ROM offset across the Option ROM EFI GOP hole.
pub const fn translate_bit_falcon_ptr(raw_ptr: usize, primary_end: usize, efi_gap_bytes: usize, rom_len: usize) -> usize {
    if raw_ptr >= primary_end && raw_ptr + efi_gap_bytes < rom_len {
        raw_ptr + efi_gap_bytes
    } else {
        raw_ptr
    }
}

/// Parse VBIOS `BIT 0x70` Falcon Ucode Table entries into `out_apps` without allocation.
pub fn parse_vbios_falcon_apps(
    rom: &[u8],
    out_apps: &mut [VbiosFalconAppEntry],
) -> Result<(VbiosSummary, usize)> {
    let summary = parse_vbios(rom)?;
    let p_end = parse_pcir_at(rom, 0)?.length;
    let tok = summary.falcon_data_token.ok_or(NvError::InvalidRomHeader)?;
    let p_data_off = translate_bit_falcon_ptr(tok.data_offset as usize, p_end, summary.efi_gap_bytes, rom.len());
    let ucode_tbl_raw = read_u32(rom, p_data_off)? as usize;
    let tbl_off = translate_bit_falcon_ptr(ucode_tbl_raw, p_end, summary.efi_gap_bytes, rom.len());

    let hdr_bytes = *rom.get(tbl_off + 1).ok_or(NvError::OutOfBounds)? as usize;
    let entry_bytes = *rom.get(tbl_off + 2).ok_or(NvError::OutOfBounds)? as usize;
    let count = *rom.get(tbl_off + 3).ok_or(NvError::OutOfBounds)? as usize;
    if hdr_bytes < 4 || entry_bytes < 6 {
        return Err(NvError::InvalidRomHeader);
    }

    let mut written = 0usize;
    for i in 0..count {
        let e_off = tbl_off + hdr_bytes + i * entry_bytes;
        let app_id = *rom.get(e_off).ok_or(NvError::OutOfBounds)?;
        let target_id = *rom.get(e_off + 1).ok_or(NvError::OutOfBounds)?;
        let raw_ptr = read_u32(rom, e_off + 2)? as usize;
        if app_id != 0 && raw_ptr != 0 {
            if written >= out_apps.len() {
                return Err(NvError::BufferTooSmall);
            }
            let adjusted = translate_bit_falcon_ptr(raw_ptr, p_end, summary.efi_gap_bytes, rom.len());
            let header_tag = read_u16(rom, adjusted)?;
            out_apps[written] = VbiosFalconAppEntry {
                app_id,
                target_id,
                adjusted_rom_offset: adjusted,
                header_tag,
            };
            written += 1;
        }
    }
    Ok((summary, written))
}

/// Locate the `JFFS` v2 InfoROM directory inside a raw `NV_PROM` image (`prom_raw.bin`)
/// and record distinct 3-byte object tags (`ROM`, `IMG`, `OBD`, `OEM`, `ECC`, `PBL`, `BBX`).
pub fn parse_prom_inforom(
    prom: &[u8],
    out_names: &mut [[u8; 3]],
) -> Result<PromInforomSummary> {
    let jffs_off = prom
        .windows(8)
        .position(|w| w == b"JFFS\x02\x00\x00\x00")
        .ok_or(NvError::InvalidRomHeader)?;
    let version = read_u32(prom, jffs_off + 4)?;
    let fs_size = read_u32(prom, jffs_off + 8)?;
    let end = prom.len().min(jffs_off.saturating_add(fs_size as usize));
    let mut pos = jffs_off + 0x80;
    let mut count = 0usize;

    while pos + 16 <= end {
        let rec_type = prom[pos];
        let tag = [prom[pos + 1], prom[pos + 2], prom[pos + 3]];
        let valid_type = matches!(rec_type, 0x01 | 0x02 | 0x04 | 0x06);
        let valid_tag = tag.iter().all(|b| b.is_ascii_alphanumeric());
        if valid_type && valid_tag {
            let mut seen = false;
            for prev in &out_names[..count] {
                if *prev == tag {
                    seen = true;
                    break;
                }
            }
            if !seen {
                if count >= out_names.len() {
                    return Err(NvError::BufferTooSmall);
                }
                out_names[count] = tag;
                count += 1;
            }
        }
        pos += 0x80;
    }

    Ok(PromInforomSummary {
        jffs_offset: jffs_off,
        version,
        fs_size,
        object_count: count,
    })
}

