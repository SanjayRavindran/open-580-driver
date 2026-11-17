//! Clean-room Pascal (`GP104`) Falcon/ACR initialization state machine.
//!
//! Replaces upstream `nova-core`'s Turing+ `Gsp::boot()` pipeline cleanly for
//! pre-Turing Pascal dies (`GP104 = 0x134`, `BootPath::PascalAcrHostRm`).

use crate::acr::{
    apply_hs_signature_patch, plan_wpr1_ls_entries, FuseStatus, HsSignaturePatch, LsImageSpec,
    LsfWprEntry,
};
use crate::falcon::{FalconDevice, FalconEngine};
use crate::fb::{FbLayout, SysmemFlush};
use crate::gpu::{BootPath, Spec};
use crate::regs::Bar0Io;
use crate::vbios::{parse_vbios, VbiosSummary};
use crate::{NvError, Result};

/// Ordered stages of the documented clean-room Pascal bring-up flow.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum BootStage {
    ProbedSpec,
    VbiosVerified,
    FbAndWpr1Planned,
    AcrHsExecuted,
    LsFalconsBootstrapped,
}

/// Inputs supplied by the OS/DRM layer for a clean-room GP104 boot cycle.
pub struct PascalBootPlan<'a> {
    pub sysmem_flush_dma_addr: u64,
    pub vbios_rom: &'a [u8],
    pub fuse: FuseStatus,
    pub wpr1_size: u64,
    pub top_reserved: u64,
    pub acr_hs_imem: &'a [u32],
    pub acr_hs_dmem: &'a mut [u32],
    pub acr_hs_sig: HsSignaturePatch,
    pub acr_boot_vec: u32,
    pub ls_images: &'a [LsImageSpec<'a>],
    pub max_polls: usize,
}

/// Result returned when Pascal ACR HS finishes locking `WPR1` and validating LS blobs.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct PascalBootReport {
    pub stage: BootStage,
    pub spec: Spec,
    pub vbios: VbiosSummary,
    pub fb_layout: FbLayout,
    pub ls_entry_count: usize,
    pub acr_mailbox1: u32,
}

/// Execute the clean-room GP104 bring-up workflow over a `Bar0Io` device.
pub fn run_pascal_bootstrap<B: Bar0Io>(
    bar0: &B,
    plan: PascalBootPlan<'_>,
) -> Result<PascalBootReport> {
    let spec = Spec::probe(bar0)?;
    if spec.boot_path != BootPath::PascalAcrHostRm {
        return Err(NvError::UnsupportedChipset(spec.chipset as u16));
    }

    let vbios = parse_vbios(plan.vbios_rom)?;
    if vbios.has_fwsec_image {
        // Pascal executes legacy VBIOS BIT DEVINIT rather than Turing+ FWSEC-FRTS.
        return Err(NvError::UnsupportedChipset(spec.chipset as u16));
    }

    SysmemFlush::flush(bar0, plan.sysmem_flush_dma_addr)?;
    let fb_layout = FbLayout::plan_wpr1(bar0, plan.wpr1_size, plan.top_reserved)?;

    let mut wpr_entries = [LsfWprEntry {
        falcon_id: 0,
        lsb_offset: 0,
        bootstrap_owner: 0,
        status: 0,
    }; 8];
    let wpr_cap = u32::try_from(plan.wpr1_size).map_err(|_| NvError::OutOfBounds)?;
    let _used_wpr = plan_wpr1_ls_entries(plan.ls_images, wpr_cap, &mut wpr_entries)?;

    apply_hs_signature_patch(plan.acr_hs_dmem, plan.fuse, &plan.acr_hs_sig)?;

    // RM runs HS ACR on PMU or SEC2 (`0x10a000` / `0x087000` on GP104) before enabling LS units.
    let sec2 = FalconDevice::new(bar0, FalconEngine::Sec2, spec.arch);
    sec2.wait_scrub(plan.max_polls)?;
    sec2.load_imem_pio(0, plan.acr_hs_imem, true)?;
    sec2.load_dmem_pio(0, plan.acr_hs_dmem)?;
    sec2.start(plan.acr_boot_vec);
    let mailbox1 = sec2.wait_halt(plan.max_polls)?;

    Ok(PascalBootReport {
        stage: BootStage::LsFalconsBootstrapped,
        spec,
        vbios,
        fb_layout,
        ls_entry_count: plan.ls_images.len(),
        acr_mailbox1: mailbox1,
    })
}

