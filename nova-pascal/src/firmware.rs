//! Clean-room `no_std` parsers for NVIDIA GP104 Falcon descriptors,
//! `192`-byte LSF signature wrappers, and packed `GR` Netlist containers.

use crate::acr::{LsFalconId, LS_SIGNATURE_BYTES};
use crate::{NvError, Result};

/// Trailing 4x `u32` XOR mask protecting the LSF metadata footer in `nv-kernel.o_binary`
/// (`[0xb0..0xc0]`: `[falcon_id, reserved0, reserved1, reserved2]`).
pub const LSF_FOOTER_XOR_MASK: [u32; 4] = [0xe931_c2b6, 0xd777_b203, 0x69a0_320e, 0x6280_4e8f];

fn read_u32_le(buf: &[u8], off: usize) -> Result<u32> {
    let bytes: [u8; 4] = buf
        .get(off..off + 4)
        .ok_or(NvError::OutOfBounds)?
        .try_into()
        .map_err(|_| NvError::OutOfBounds)?;
    Ok(u32::from_le_bytes(bytes))
}

/// 24-byte Falcon bootloader descriptor (`pmu_bl`, `sec2_bl`, `fecs_bl_desc`, `gpccs_bl_desc`).
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct FlcnBlDesc {
    pub start_tag: u32,
    pub boot_vec: u32,
    pub code_size: u32,
    pub data_off: u32,
    pub data_size: u32,
}

impl FlcnBlDesc {
    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.len() != 24 {
            return Err(NvError::InvalidRomHeader);
        }
        let start_tag = read_u32_le(data, 0)?;
        Ok(Self {
            start_tag,
            boot_vec: start_tag.checked_shl(8).ok_or(NvError::OutOfBounds)?,
            code_size: read_u32_le(data, 12)?,
            data_off: read_u32_le(data, 16)?,
            data_size: read_u32_le(data, 20)?,
        })
    }
}

/// 36-byte HS ACR ucode descriptor (`id_0x18` in ACR bindata archives).
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct AcrHsDesc {
    pub version: u32,
    pub imem_off: u32,
    pub imem_size: u32,
    pub dmem_size: u32,
    pub num_apps: u32,
    pub app_code_off: u32,
    pub app_code_size: u32,
}

impl AcrHsDesc {
    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.len() != 36 {
            return Err(NvError::InvalidRomHeader);
        }
        Ok(Self {
            version: read_u32_le(data, 0)?,
            imem_off: read_u32_le(data, 4)?,
            imem_size: read_u32_le(data, 8)?,
            dmem_size: read_u32_le(data, 12)?,
            num_apps: read_u32_le(data, 16)?,
            app_code_off: read_u32_le(data, 20)?,
            app_code_size: read_u32_le(data, 24)?,
        })
    }
}

/// Decoded RTOS Falcon header (`PMU` `940B` / `SEC2` `656B`, `desc_size == 0x290`).
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct RtosDesc {
    pub desc_size: u32,
    pub stored_size: u32,
    pub flags: u32,
    pub changelist: u32,
    pub app_start_addr: u32,
    pub app_entry: u32,
    pub code_offset: u32,
    pub data_offset_span: u32,
    pub total_expected_image_bytes: u32,
    pub num_overlays: u32,
}

impl RtosDesc {
    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.len() < 0x90 {
            return Err(NvError::InvalidRomHeader);
        }
        let flags = read_u32_le(data, 8)?;
        let base = if flags <= 2 { 0x54 } else { 0x58 };
        let app_start_addr = read_u32_le(data, base + 4)?;
        let app_entry = read_u32_le(data, base + 8)?;
        let code_offset = read_u32_le(data, base + 12)?;
        let data_offset_span = read_u32_le(data, base + 16)?;
        Ok(Self {
            desc_size: read_u32_le(data, 0)?,
            stored_size: read_u32_le(data, 4)?,
            flags,
            changelist: read_u32_le(data, 12)?,
            app_start_addr,
            app_entry,
            code_offset,
            data_offset_span,
            total_expected_image_bytes: code_offset
                .checked_add(data_offset_span)
                .ok_or(NvError::OutOfBounds)?,
            num_overlays: read_u32_le(data, 0x88)?,
        })
    }
}

/// Decoded metadata from a `192`-byte LS Falcon signature wrapper.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct LsfSigInfo {
    pub falcon_id: LsFalconId,
}

impl LsfSigInfo {
    /// Decode `192`-byte `LSF_UCODE_DESC` wrapper by validating `[160..176]` zero
    /// padding and unmasking `[176..192]` with `LSF_FOOTER_XOR_MASK`.
    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.len() != LS_SIGNATURE_BYTES {
            return Err(NvError::SignatureFuseMismatch);
        }
        for i in 0..4 {
            if read_u32_le(data, 160 + i * 4)? != 0 {
                return Err(NvError::SignatureFuseMismatch);
            }
        }
        let raw_fid = read_u32_le(data, 176)? ^ LSF_FOOTER_XOR_MASK[0];
        let chk1 = read_u32_le(data, 180)? ^ LSF_FOOTER_XOR_MASK[1];
        if chk1 != 0 {
            return Err(NvError::SignatureFuseMismatch);
        }
        let falcon_id = LsFalconId::from_u32(raw_fid).ok_or(NvError::SignatureFuseMismatch)?;
        Ok(Self { falcon_id })
    }
}

/// Packed `GR` Netlist TOC section record (`12` bytes each inside `gp104_gr_netlist.bin`).
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct GrNetlistSection {
    pub section_type: u32,
    pub size: u32,
    pub offset: u32,
}

/// Parse packed `GR` Netlist header & section directory (`magic == 0`, e.g., `32` entries).
pub fn parse_gr_netlist_toc(data: &[u8], out: &mut [GrNetlistSection]) -> Result<usize> {
    if data.len() < 8 {
        return Err(NvError::InvalidRomHeader);
    }
    let magic = read_u32_le(data, 0)?;
    let count = read_u32_le(data, 4)? as usize;
    if magic != 0 || count == 0 || count > 64 {
        return Err(NvError::InvalidRomHeader);
    }
    if 8 + count * 12 > data.len() {
        return Err(NvError::OutOfBounds);
    }
    if out.len() < count {
        return Err(NvError::BufferTooSmall);
    }
    for (idx, dst) in out[..count].iter_mut().enumerate() {
        let base = 8 + idx * 12;
        let sec_type = read_u32_le(data, base)?;
        let size = read_u32_le(data, base + 4)?;
        let offset = read_u32_le(data, base + 8)?;
        if (offset as usize).saturating_add(size as usize) > data.len() {
            return Err(NvError::OutOfBounds);
        }
        *dst = GrNetlistSection {
            section_type: sec_type,
            size,
            offset,
        };
    }
    Ok(count)
}

