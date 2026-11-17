//! Framebuffer sizing, NISO sysmem flush, and WPR1 placement matching `nova-core`.

use crate::regs::{
    Bar0Io, NV_PFB_NISO_FLUSH_SYSMEM_ADDR, NV_PFB_NISO_FLUSH_SYSMEM_ADDR_HI,
    NV_PFB_PRI_MMU_LOCAL_MEMORY_RANGE, NV_PFB_PRI_MMU_WPR_INFO,
};
use crate::{NvError, Result};

/// Page alignment required by `NV_PFB_NISO_FLUSH_SYSMEM_ADDR` (8-bit right shift = 256B,
/// typically allocated as a 4 KiB coherent DMA page).
pub const SYSMEM_FLUSH_SHIFT: u32 = 8;

/// Hardware requirement: `WPR1` start/end addresses are aligned to `128 KiB` (`0x2_0000`).
pub const WPR1_ALIGN_BYTES: u64 = 128 * 1024;

/// Decoded VRAM geometry and Write-Protected Region (`WPR1`) carving.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct FbLayout {
    pub vidmem_bytes: u64,
    pub wpr1_start: u64,
    pub wpr1_end: u64,
    pub ecc_enabled: bool,
}

impl FbLayout {
    /// Decode usable VRAM size from `NV_PFB_PRI_MMU_LOCAL_MEMORY_RANGE` (`0x100ce0`).
    ///
    /// Format on Pascal+:
    /// - `[3:0]` `LOWER_RANGE_SCALE`
    /// - `[9:4]` `LOWER_RANGE_MAG`
    /// - `[30]`  `ECC_MODE`
    pub fn vidmem_size_from_reg(range_reg: u32) -> Result<u64> {
        let scale = range_reg & 0x0f;
        let mag = ((range_reg >> 4) & 0x3f) as u64;
        if mag == 0 || scale > 24 {
            return Err(NvError::OutOfBounds);
        }
        let shift = (scale as u32) + 20;
        mag.checked_shl(shift).ok_or(NvError::OutOfBounds)
    }

    /// Compute aligned `WPR1` boundaries near the top of VRAM (excluding optional
    /// VBIOS/FRTS reservation at the very end if present).
    pub fn plan_wpr1<B: Bar0Io>(
        bar0: &B,
        wpr1_size: u64,
        top_reserved: u64,
    ) -> Result<Self> {
        if wpr1_size == 0 || wpr1_size % WPR1_ALIGN_BYTES != 0 {
            return Err(NvError::InvalidAlignment);
        }
        let reg = bar0.read32(NV_PFB_PRI_MMU_LOCAL_MEMORY_RANGE);
        let vidmem_bytes = Self::vidmem_size_from_reg(reg)?;
        let ecc_enabled = ((reg >> 30) & 1) != 0;

        let usable_top = vidmem_bytes
            .checked_sub(top_reserved)
            .ok_or(NvError::OutOfBounds)?;
        let aligned_end = usable_top & !(WPR1_ALIGN_BYTES - 1);
        let wpr1_start = aligned_end
            .checked_sub(wpr1_size)
            .ok_or(NvError::OutOfBounds)?;

        Ok(Self {
            vidmem_bytes,
            wpr1_start,
            wpr1_end: aligned_end,
            ecc_enabled,
        })
    }

    /// Read current hardware `WPR_INFO` index register (`0x100e00`).
    pub fn read_wpr_info<B: Bar0Io>(bar0: &B) -> u32 {
        bar0.read32(NV_PFB_PRI_MMU_WPR_INFO)
    }
}

/// Issues a BAR0-to-sysmem NISO write-flush cycle (`SysmemFlush` in `nova-core`).
pub struct SysmemFlush;

impl SysmemFlush {
    /// Program the backing DMA page physical address (`dma_addr`) and trigger flush.
    pub fn flush<B: Bar0Io>(bar0: &B, dma_addr: u64) -> Result<()> {
        if dma_addr == 0 || (dma_addr & 0xff) != 0 {
            return Err(NvError::InvalidAlignment);
        }
        let lo = ((dma_addr >> SYSMEM_FLUSH_SHIFT) & 0xffff_ffff) as u32;
        let hi = ((dma_addr >> (SYSMEM_FLUSH_SHIFT + 32)) & 0xff) as u32;
        bar0.write32(NV_PFB_NISO_FLUSH_SYSMEM_ADDR_HI, hi);
        bar0.write32(NV_PFB_NISO_FLUSH_SYSMEM_ADDR, lo);
        let _ = bar0.read32(NV_PFB_NISO_FLUSH_SYSMEM_ADDR);
        Ok(())
    }
}

