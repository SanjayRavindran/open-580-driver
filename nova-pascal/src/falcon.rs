//! Falcon microcontroller register HAL and PIO IMEM/DMEM loader for `GP104`.

use crate::gpu::Architecture;
use crate::regs::Bar0Io;
use crate::{NvError, Result};

// Standard PFALCON register offsets relative to engine base.
pub const FALCON_IRQSCLR: usize = 0x004;
pub const FALCON_IRQMASK: usize = 0x010;
pub const FALCON_MAILBOX0: usize = 0x040;
pub const FALCON_MAILBOX1: usize = 0x044;
pub const FALCON_CPUCTL: usize = 0x100;
pub const FALCON_BOOTVEC: usize = 0x104;
pub const FALCON_HWCFG: usize = 0x108;
pub const FALCON_DMACTL: usize = 0x10c;
pub const FALCON_IMEMC0: usize = 0x180;
pub const FALCON_IMEMD0: usize = 0x184;
pub const FALCON_IMEMT0: usize = 0x188;
pub const FALCON_DMEMC0: usize = 0x1c0;
pub const FALCON_DMEMD0: usize = 0x1c4;
/// Falcon security status & privilege mode register (`NV_PFALCON_FALCON_SCTL`).
///
/// Observed values on GP104 silicon (`0x1bb3`):
/// - `0x0000_3000`: Unauthenticated (`nouveau` / no driver / stopped non-LS)
/// - `0x0000_7021`: Light-Secure (`LS`) authenticated mode under R580 (`PMU`, `SEC2`, `FECS`, `GPCCS`)
pub const FALCON_SCTL: usize = 0x240;
pub const SCTL_UNTRUSTED_IDLE: u32 = 0x0000_3000;
pub const SCTL_LS_ACTIVE: u32 = 0x0000_7021;

// CPUCTL bits.
pub const CPUCTL_STARTCPU: u32 = 1 << 1;
pub const CPUCTL_HALTED: u32 = 1 << 4;

// DMACTL bits (scrubbing done when both IMEM & DMEM scrubbing bits clear).
pub const DMACTL_DMEM_SCRUBBING: u32 = 1 << 1;
pub const DMACTL_IMEM_SCRUBBING: u32 = 1 << 2;

// IMEMC / DMEMC control bits (auto-increment on write + secure tag mode).
const MEMC_AINCW: u32 = 1 << 24;
const IMEMC_SECURE: u32 = 1 << 28;

/// Silicon Falcon engine units present on Pascal (`GP104`) and Turing+.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum FalconEngine {
    Pmu,
    Sec2,
    Nvdec,
    Nvenc0,
    Fecs,
    Gpccs,
}

impl FalconEngine {
    /// Return BAR0 base offset for the target microarchitecture.
    ///
    /// Crucial hardware deltas observed against `nv-kernel.o_binary` & live silicon:
    /// - Pascal (`GP104`) places `SEC2` at `0x0008_7000` (Turing+ `nova-core` uses `0x0084_0000`).
    /// - Pascal (`GP104`) exposes primary `NVENC0` at `0x001c_8000` (`_nv046356rm`).
    pub const fn bar0_base(self, arch: Architecture) -> usize {
        match (self, arch) {
            (Self::Pmu, _) => 0x0010_a000,
            (Self::Nvdec, _) => 0x0008_4000,
            (Self::Nvenc0, _) => 0x001c_8000,
            (Self::Fecs, _) => 0x0040_9000,
            (Self::Gpccs, _) => 0x0041_a000,
            (Self::Sec2, Architecture::Pascal | Architecture::Volta) => 0x0008_7000,
            (Self::Sec2, Architecture::Turing | Architecture::Ampere | Architecture::Ada) => {
                0x0084_0000
            }
        }
    }
}

/// Bound Falcon register access helper for a single engine instance.
pub struct FalconDevice<'a, B: Bar0Io> {
    bar0: &'a B,
    base: usize,
}

impl<'a, B: Bar0Io> FalconDevice<'a, B> {
    pub const fn new(bar0: &'a B, engine: FalconEngine, arch: Architecture) -> Self {
        Self {
            bar0,
            base: engine.bar0_base(arch),
        }
    }

    pub const fn base(&self) -> usize {
        self.base
    }

    pub fn read_cpuctl(&self) -> u32 {
        self.rd(FALCON_CPUCTL)
    }

    pub fn read_bootvec(&self) -> u32 {
        self.rd(FALCON_BOOTVEC)
    }

    pub fn read_sctl(&self) -> u32 {
        self.rd(FALCON_SCTL)
    }

    /// Return `true` when silicon reports Light-Secure authenticated execution (`0x7021`).
    pub fn is_ls_mode(&self) -> bool {
        self.read_sctl() == SCTL_LS_ACTIVE
    }

    fn rd(&self, reg: usize) -> u32 {
        self.bar0.read32(self.base + reg)
    }

    fn wr(&self, reg: usize, val: u32) {
        self.bar0.write32(self.base + reg, val);
    }

    /// Mask interrupts and wait for hardware IMEM/DMEM SRAM scrubbing to finish.
    pub fn wait_scrub(&self, max_polls: usize) -> Result<()> {
        self.wr(FALCON_IRQMASK, 0);
        self.wr(FALCON_IRQSCLR, u32::MAX);
        for _ in 0..max_polls {
            let dma_ctl = self.rd(FALCON_DMACTL);
            if (dma_ctl & (DMACTL_IMEM_SCRUBBING | DMACTL_DMEM_SCRUBBING)) == 0 {
                return Ok(());
            }
        }
        Err(NvError::IoTimeout)
    }

    /// Write instruction words to Falcon IMEM via PIO auto-increment port.
    ///
    /// Note: Every 256-byte (`64`-word) IMEM block requires a matching tag in
    /// `IMEMT0` (`block_idx = byte_off >> 8`).
    pub fn load_imem_pio(&self, dst_offset: u32, words: &[u32], secure: bool) -> Result<()> {
        if (dst_offset & 3) != 0 {
            return Err(NvError::InvalidAlignment);
        }
        let sec_bit = if secure { IMEMC_SECURE } else { 0 };
        self.wr(FALCON_IMEMC0, (dst_offset & 0xffff) | MEMC_AINCW | sec_bit);
        let start_word = (dst_offset / 4) as usize;
        for (i, &w) in words.iter().enumerate() {
            let word_idx = start_word + i;
            if (word_idx % 64) == 0 || i == 0 {
                let tag = (word_idx / 64) as u32;
                self.wr(FALCON_IMEMT0, tag);
            }
            self.wr(FALCON_IMEMD0, w);
        }
        Ok(())
    }

    /// Write data words to Falcon DMEM via PIO auto-increment port.
    pub fn load_dmem_pio(&self, dst_offset: u32, words: &[u32]) -> Result<()> {
        if (dst_offset & 3) != 0 {
            return Err(NvError::InvalidAlignment);
        }
        self.wr(FALCON_DMEMC0, (dst_offset & 0xffff) | MEMC_AINCW);
        for &w in words {
            self.wr(FALCON_DMEMD0, w);
        }
        Ok(())
    }

    /// Program `BOOTVEC` and pulse `CPUCTL.STARTCPU`.
    pub fn start(&self, boot_vec: u32) {
        self.wr(FALCON_MAILBOX0, 0);
        self.wr(FALCON_BOOTVEC, boot_vec);
        self.wr(FALCON_CPUCTL, CPUCTL_STARTCPU);
    }

    /// Poll `CPUCTL.HALTED` and verify `MAILBOX0` returned zero.
    ///
    /// Observed RM error behavior (`_nv016743rm`): Falcon HS BROM / ACR returns
    /// non-zero status codes in `MAILBOX0` (e.g. `0x4f` on HS/LS signature failure).
    pub fn wait_halt(&self, max_polls: usize) -> Result<u32> {
        for _ in 0..max_polls {
            let cpuctl = self.rd(FALCON_CPUCTL);
            if (cpuctl & CPUCTL_HALTED) != 0 {
                let mb0 = self.rd(FALCON_MAILBOX0);
                if mb0 != 0 {
                    return Err(NvError::FalconErrorCode(mb0));
                }
                return Ok(self.rd(FALCON_MAILBOX1));
            }
        }
        Err(NvError::IoTimeout)
    }
}

