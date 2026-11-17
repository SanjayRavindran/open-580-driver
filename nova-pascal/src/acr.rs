//! Clean-room Access Control Region (`WPR1`) & signed Falcon descriptor builder.
//!
//! Host software NEVER computes or bypasses Falcon signatures; it enforces the
//! silicon `PrivSec` predicate, picks between NVIDIA's pre-signed Production or
//! Debug HS signatures, and arranges WPR1 metadata headers for the HS ACR binary.

use crate::regs::{Bar0Io, NV_FUSE_PRIV_SEC_STATUS};
use crate::{NvError, Result};

/// Hardware fused security state checked prior to HS/LS Falcon upload.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum FuseMode {
    Production,
    Debug,
}

/// Silicon fuse gate evaluated by the host driver (`_nv030951rm`, `FUN_00104034`).
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct FuseStatus {
    /// Silicon `PrivSec` enabled state (`true` required for HS/LS ACR execution).
    pub priv_sec_enabled: bool,
    pub mode: FuseMode,
}

impl FuseStatus {
    /// Probe fused `PrivSec` state directly from `NV_FUSE` (`0x21400`, observed `0x1` on GP104).
    pub fn probe<B: Bar0Io>(bar0: &B, mode: FuseMode) -> Self {
        let priv_sec_enabled = (bar0.read32(NV_FUSE_PRIV_SEC_STATUS) & 1) != 0;
        Self {
            priv_sec_enabled,
            mode,
        }
    }
}

/// Metadata extracted from `580.178.04` HS Falcon bindata archives (e.g. `gp104_acr_load`).
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct HsSignaturePatch {
    /// Byte offset inside DMEM where the 16-byte HS signature must be patched
    /// (observed `0x3a20` in `gp104_acr_load/id_0x3b.bin`).
    pub patch_loc: u32,
    pub prod_sig: [u32; 4],
    pub dbg_sig: [u32; 4],
}

/// Select the NVIDIA-signed `PROD` or `DBG` 16-byte HS signature and patch
/// DMEM words at `patch_loc` before launching the Falcon (`_nv016905rm`).
pub fn apply_hs_signature_patch(
    dmem_words: &mut [u32],
    fuse: FuseStatus,
    patch: &HsSignaturePatch,
) -> Result<()> {
    if !fuse.priv_sec_enabled {
        return Err(NvError::PrivSecDisabled);
    }
    if (patch.patch_loc & 3) != 0 {
        return Err(NvError::InvalidAlignment);
    }
    let word_off = (patch.patch_loc / 4) as usize;
    let dst = dmem_words
        .get_mut(word_off..word_off + 4)
        .ok_or(NvError::OutOfBounds)?;
    let sig = match fuse.mode {
        FuseMode::Production => &patch.prod_sig,
        FuseMode::Debug => &patch.dbg_sig,
    };
    dst.copy_from_slice(sig);
    Ok(())
}

/// Light-Secure (LS) Falcon engine IDs managed inside `WPR1` (`_nv029066rm` / XOR footer).
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u32)]
pub enum LsFalconId {
    Pmu = 0,
    Fecs = 2,
    Gpccs = 3,
    Nvenc = 4,
    Nvdec = 5,
    Sec2 = 7,
}

impl LsFalconId {
    /// Convert a decoded numeric Falcon engine ID into `LsFalconId`.
    pub const fn from_u32(val: u32) -> Option<Self> {
        match val {
            0 => Some(Self::Pmu),
            2 => Some(Self::Fecs),
            3 => Some(Self::Gpccs),
            4 => Some(Self::Nvenc),
            5 => Some(Self::Nvdec),
            7 => Some(Self::Sec2),
            _ => None,
        }
    }
}

/// Light-Secure signature size verified across GP104 `PMU`, `SEC2`, `FECS`, and `GPCCS`.
pub const LS_SIGNATURE_BYTES: usize = 192;

/// Descriptor of one signed LS firmware image placed into `WPR1`.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct LsImageSpec<'a> {
    pub falcon_id: LsFalconId,
    pub ucode_image: &'a [u8],
    pub ls_signature: &'a [u8],
    pub bootloader_size: u32,
    pub app_code_offset: u32,
    pub app_code_size: u32,
    pub app_data_offset: u32,
    pub app_data_size: u32,
}

/// Computed `WPR1` placement entry for a verified LS Falcon image.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct LsfWprEntry {
    pub falcon_id: u32,
    pub lsb_offset: u32,
    pub bootstrap_owner: u32,
    pub status: u32,
}

fn align_up(value: u32, align: u32) -> Result<u32> {
    if !align.is_power_of_two() {
        return Err(NvError::InvalidAlignment);
    }
    let mask = align - 1;
    value
        .checked_add(mask)
        .map(|v| v & !mask)
        .ok_or(NvError::OutOfBounds)
}

/// Validate LS blobs (including mandatory `192`-byte NVIDIA LS signature) and
/// lay out `LsfWprEntry` offsets inside `WPR1`.
pub fn plan_wpr1_ls_entries(
    images: &[LsImageSpec<'_>],
    wpr_capacity_bytes: u32,
    out_entries: &mut [LsfWprEntry],
) -> Result<u32> {
    if out_entries.len() < images.len() {
        return Err(NvError::BufferTooSmall);
    }
    // Reserve first 4 KiB of WPR1 for LSF_WPR_HEADER array + LSB headers.
    let mut cursor: u32 = 0x1000;
    for (idx, img) in images.iter().enumerate() {
        if img.ucode_image.is_empty() || img.ls_signature.len() != LS_SIGNATURE_BYTES {
            return Err(NvError::SignatureFuseMismatch);
        }
        let img_len = u32::try_from(img.ucode_image.len()).map_err(|_| NvError::OutOfBounds)?;
        cursor = align_up(cursor, 256)?;
        let next = cursor.checked_add(img_len).ok_or(NvError::OutOfBounds)?;
        if next > wpr_capacity_bytes {
            return Err(NvError::OutOfBounds);
        }
        out_entries[idx] = LsfWprEntry {
            falcon_id: img.falcon_id as u32,
            lsb_offset: cursor,
            bootstrap_owner: LsFalconId::Sec2 as u32,
            status: 1,
        };
        cursor = next;
    }
    Ok(cursor)
}

