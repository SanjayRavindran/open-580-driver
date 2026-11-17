//! Core BAR0 MMIO abstraction and verified Pascal/GP104 register offsets.

/// Read/write interface over PCI BAR0 register space (`16 MiB` on `GP104`).
///
/// Matches the clean MMIO boundary used in Linux `nova-core`, allowing unit
/// tests to supply an in-memory mock BAR0 register file without hardware access.
pub trait Bar0Io {
    /// Read a 32-bit little-endian register at byte `offset` within BAR0.
    fn read32(&self, offset: usize) -> u32;

    /// Write a 32-bit little-endian register at byte `offset` within BAR0.
    fn write32(&self, offset: usize, value: u32);
}

/// Primary GPU identification register (`NV_PMC_BOOT_0`).
pub const NV_PMC_BOOT_0: usize = 0x0000_0000;

/// Extended GPU architecture and implementation register (`NV_PMC_BOOT_42`).
///
/// Bits `[29:24]` encode architecture (e.g. `0x13` = Pascal) and bits `[23:20]`
/// encode implementation (`0x4` = GP104).
pub const NV_PMC_BOOT_42: usize = 0x0000_0a00;

/// Base of `NV_PBUS_VBIOS_SCRATCH` registers used for DEVINIT / FRTS status.
pub const NV_PBUS_VBIOS_SCRATCH_BASE: usize = 0x0000_1400;

/// Hardware fuse status window (`NV_FUSE` control & status).
pub const NV_FUSE_STATUS: usize = 0x0002_1000;

/// Hardware fused Production Secure Boot register (`NV_FUSE` `PrivSec` bit `0`, observed `0x1` on GP104).
pub const NV_FUSE_PRIV_SEC_STATUS: usize = 0x0002_1400;

/// On-chip SPI PROM/VBIOS+InfoROM aperture (`NV_PROM_DATA`, `256 KiB` on GP104).
pub const NV_PROM_DATA_BASE: usize = 0x0030_0000;

/// System memory NISO flush target address register (`SysmemFlush` low 32 bits).
pub const NV_PFB_NISO_FLUSH_SYSMEM_ADDR: usize = 0x0010_0c10;

/// Extended NISO flush high address register (upper DMA bits).
pub const NV_PFB_NISO_FLUSH_SYSMEM_ADDR_HI: usize = 0x0010_0c40;

/// Local VRAM sizing register (`NV_PFB_PRI_MMU_LOCAL_MEMORY_RANGE`).
pub const NV_PFB_PRI_MMU_LOCAL_MEMORY_RANGE: usize = 0x0010_0ce0;

/// Write-Protected Region (`WPR1` / `WPR2`) hardware status register (`NV_PFB_PRI_MMU_WPR_INFO`).
pub const NV_PFB_PRI_MMU_WPR_INFO: usize = 0x0010_0e00;

/// Start of WPR/MMU `PRIV_LEVEL_MASK` protected control window (`0x100e40..0x100e50`).
pub const NV_PFB_PRI_MMU_WPR_PRIV_BASE: usize = 0x0010_0e40;

/// Hardware PRI bus error tag returned when host Level-0 reads a Level-2/3 protected register (`0xbadf5040`).
pub const NV_PRI_ERROR_MASK: u32 = 0xffff_0000;
pub const NV_PRI_BADF_PREFIX: u32 = 0xbadf_0000;
pub const NV_PRI_PRIV_BLOCKED_WPR: u32 = 0xbadf_5040;

/// Return `true` if a MMIO read returned a PRI timeout / `PRIV_LEVEL_MASK` fault (`0xbadfxxxx`).
pub const fn is_pri_error(value: u32) -> bool {
    (value & NV_PRI_ERROR_MASK) == NV_PRI_BADF_PREFIX
}

