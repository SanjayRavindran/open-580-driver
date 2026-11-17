//! GPU identity decoding matching `nova-core` `Architecture`, `Chipset`, and `Spec`.

use crate::regs::{Bar0Io, NV_PMC_BOOT_0, NV_PMC_BOOT_42};
use crate::{NvError, Result};

/// GPU microarchitecture family encoded in `NV_PMC_BOOT_42[29:24]`.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u8)]
pub enum Architecture {
    /// Pascal (`GP10x`, no RISC-V GSP — host RM + Falcon ACR/PMU/SEC2).
    Pascal = 0x13,
    /// Volta (`GV100`).
    Volta = 0x14,
    /// Turing (`TU10x`/`TU11x`, first generation supported by upstream `nova-core`).
    Turing = 0x16,
    /// Ampere (`GA10x`).
    Ampere = 0x17,
    /// Ada Lovelace (`AD10x`).
    Ada = 0x19,
}

impl Architecture {
    /// Decode `Architecture` from raw 6-bit value.
    pub const fn from_raw(raw: u8) -> Option<Self> {
        match raw {
            0x13 => Some(Self::Pascal),
            0x14 => Some(Self::Volta),
            0x16 => Some(Self::Turing),
            0x17 => Some(Self::Ampere),
            0x19 => Some(Self::Ada),
            _ => None,
        }
    }
}

/// Specific GPU die encoded as `(arch << 4) | implementation`.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u16)]
pub enum Chipset {
    GP100 = 0x130,
    GP102 = 0x132,
    /// Target die for NVIDIA Tesla P4 (`GP104GL`, PCI `10de:1bb3`).
    GP104 = 0x134,
    GP106 = 0x136,
    GP107 = 0x137,
    GP108 = 0x138,
    TU102 = 0x162,
    TU104 = 0x164,
    GA102 = 0x172,
    AD102 = 0x192,
}

/// Bootstrap architecture required by the silicon.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum BootPath {
    /// Pre-Turing: VBIOS DEVINIT wait + host WPR1 setup + HS ACR on PMU/SEC2 + host RM.
    PascalAcrHostRm,
    /// Turing+: `FWSEC-FRTS` -> `Booter` -> RISC-V `GSP-RM` (upstream `nova-core` default).
    TuringPlusGsp,
}

struct ChipEntry {
    raw_id: u16,
    chipset: Chipset,
    arch: Architecture,
    rm_tag: Option<u32>,
    boot_path: BootPath,
}

const CHIP_TABLE: &[ChipEntry] = &[
    ChipEntry { raw_id: 0x130, chipset: Chipset::GP100, arch: Architecture::Pascal, rm_tag: Some(0x1d), boot_path: BootPath::PascalAcrHostRm },
    ChipEntry { raw_id: 0x132, chipset: Chipset::GP102, arch: Architecture::Pascal, rm_tag: Some(0x1e), boot_path: BootPath::PascalAcrHostRm },
    ChipEntry { raw_id: 0x134, chipset: Chipset::GP104, arch: Architecture::Pascal, rm_tag: Some(0x1f), boot_path: BootPath::PascalAcrHostRm },
    ChipEntry { raw_id: 0x136, chipset: Chipset::GP106, arch: Architecture::Pascal, rm_tag: Some(0x20), boot_path: BootPath::PascalAcrHostRm },
    ChipEntry { raw_id: 0x137, chipset: Chipset::GP107, arch: Architecture::Pascal, rm_tag: Some(0x21), boot_path: BootPath::PascalAcrHostRm },
    ChipEntry { raw_id: 0x138, chipset: Chipset::GP108, arch: Architecture::Pascal, rm_tag: Some(0x22), boot_path: BootPath::PascalAcrHostRm },
    ChipEntry { raw_id: 0x162, chipset: Chipset::TU102, arch: Architecture::Turing, rm_tag: Some(0x25), boot_path: BootPath::TuringPlusGsp },
    ChipEntry { raw_id: 0x164, chipset: Chipset::TU104, arch: Architecture::Turing, rm_tag: Some(0x26), boot_path: BootPath::TuringPlusGsp },
    ChipEntry { raw_id: 0x172, chipset: Chipset::GA102, arch: Architecture::Ampere, rm_tag: None, boot_path: BootPath::TuringPlusGsp },
    ChipEntry { raw_id: 0x192, chipset: Chipset::AD102, arch: Architecture::Ada, rm_tag: None, boot_path: BootPath::TuringPlusGsp },
];

/// Decoded hardware revision and chipset specification (`Spec` in `nova-core`).
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Spec {
    pub chipset: Chipset,
    pub arch: Architecture,
    pub major_rev: u8,
    pub minor_rev: u8,
    /// Internal RM HAL dispatch tag (`_nv006052rm`, e.g., `0x1f` for `GP104`).
    pub rm_tag: Option<u32>,
    pub boot_path: BootPath,
}

impl Spec {
    /// Decode GPU identity from raw `NV_PMC_BOOT_0` and `NV_PMC_BOOT_42` register values.
    pub fn from_boot(boot0: u32, boot42: u32) -> Result<Self> {
        let arch_bits = ((boot42 >> 24) & 0x3f) as u8;
        let impl_bits = ((boot42 >> 20) & 0x0f) as u8;
        let major_rev = ((boot42 >> 16) & 0x0f) as u8;
        let minor_rev = ((boot42 >> 12) & 0x0f) as u8;

        // BOOT_0 must not be all-ones (disconnected PCIe link) or zero.
        if boot0 == 0 || boot0 == u32::MAX {
            return Err(NvError::UnsupportedChipset(0));
        }

        let raw_id = ((arch_bits as u16) << 4) | (impl_bits as u16);
        for entry in CHIP_TABLE {
            if entry.raw_id == raw_id {
                return Ok(Self {
                    chipset: entry.chipset,
                    arch: entry.arch,
                    major_rev,
                    minor_rev,
                    rm_tag: entry.rm_tag,
                    boot_path: entry.boot_path,
                });
            }
        }
        Err(NvError::UnsupportedChipset(raw_id))
    }

    /// Probe GPU specification directly from a `Bar0Io` MMIO mapping.
    pub fn probe<B: Bar0Io>(bar0: &B) -> Result<Self> {
        let boot0 = bar0.read32(NV_PMC_BOOT_0);
        let boot42 = bar0.read32(NV_PMC_BOOT_42);
        Self::from_boot(boot0, boot42)
    }
}

