//! Clean-room `no_std` Pascal (`GP104`) Falcon/ACR building blocks modeled on Linux `nova-core`.

#![cfg_attr(not(test), no_std)]

pub mod acr;
pub mod boot;
pub mod falcon;
pub mod fb;
pub mod firmware;
pub mod gpu;
pub mod regs;
pub mod vbios;

/// Unified clean-room driver error status matching silicon failure boundaries.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum NvError {
    IoTimeout,
    UnsupportedChipset(u16),
    InvalidRomHeader,
    SignatureFuseMismatch,
    PrivSecDisabled,
    /// Silicon Falcon BROM/HS returned a non-zero `MAILBOX0` error (e.g. `0x4f`).
    FalconErrorCode(u32),
    BufferTooSmall,
    OutOfBounds,
    InvalidAlignment,
}

pub type Result<T> = core::result::Result<T, NvError>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acr::{FuseMode, FuseStatus, HsSignaturePatch, LsFalconId, LsImageSpec};
    use crate::boot::{run_pascal_bootstrap, BootStage, PascalBootPlan};
    use crate::falcon::{
        FalconEngine, CPUCTL_HALTED, CPUCTL_STARTCPU, FALCON_BOOTVEC, FALCON_CPUCTL,
        FALCON_DMACTL, FALCON_DMEMD0, FALCON_IMEMD0, FALCON_MAILBOX0, FALCON_MAILBOX1,
    };
    use crate::gpu::{Architecture, BootPath, Chipset, Spec};
    use crate::regs::{
        Bar0Io, NV_PFB_NISO_FLUSH_SYSMEM_ADDR, NV_PFB_PRI_MMU_LOCAL_MEMORY_RANGE, NV_PMC_BOOT_0,
        NV_PMC_BOOT_42,
    };
    use core::cell::RefCell;
    use std::collections::BTreeMap;

    struct MockBar0 {
        regs: RefCell<BTreeMap<usize, u32>>,
        imem_words: RefCell<Vec<u32>>,
        dmem_words: RefCell<Vec<u32>>,
        sec2_base: usize,
        mailbox0_on_start: u32,
        mailbox1_on_start: u32,
        never_halt: bool,
    }

    impl MockBar0 {
        fn new_gp104() -> Self {
            let mut map = BTreeMap::new();
            // GP104 BOOT_0 & BOOT_42 (arch=0x13, impl=0x4, rev A1)
            map.insert(NV_PMC_BOOT_0, 0x1340_00a1);
            map.insert(NV_PMC_BOOT_42, (0x13 << 24) | (0x4 << 20) | (0xa << 16) | (1 << 12));
            // 8 GiB VRAM: mag=0x20 (32), scale=0x8 -> 32 << 28 = 8 GiB
            map.insert(NV_PFB_PRI_MMU_LOCAL_MEMORY_RANGE, (0x20 << 4) | 0x8);
            Self {
                regs: RefCell::new(map),
                imem_words: RefCell::new(Vec::new()),
                dmem_words: RefCell::new(Vec::new()),
                sec2_base: FalconEngine::Sec2.bar0_base(Architecture::Pascal),
                mailbox0_on_start: 0,
                mailbox1_on_start: 0x0000_0001,
                never_halt: false,
            }
        }
    }

    impl Bar0Io for MockBar0 {
        fn read32(&self, offset: usize) -> u32 {
            *self.regs.borrow().get(&offset).unwrap_or(&0)
        }

        fn write32(&self, offset: usize, value: u32) {
            self.regs.borrow_mut().insert(offset, value);
            if offset == self.sec2_base + FALCON_IMEMD0 {
                self.imem_words.borrow_mut().push(value);
            } else if offset == self.sec2_base + FALCON_DMEMD0 {
                self.dmem_words.borrow_mut().push(value);
            } else if offset == self.sec2_base + FALCON_CPUCTL && (value & CPUCTL_STARTCPU) != 0 {
                self.regs
                    .borrow_mut()
                    .insert(self.sec2_base + FALCON_MAILBOX0, self.mailbox0_on_start);
                self.regs
                    .borrow_mut()
                    .insert(self.sec2_base + FALCON_MAILBOX1, self.mailbox1_on_start);
                if !self.never_halt {
                    self.regs
                        .borrow_mut()
                        .insert(self.sec2_base + FALCON_CPUCTL, CPUCTL_HALTED);
                }
            } else if offset == self.sec2_base + FALCON_DMACTL {
                // Scrub finishes immediately in mock hardware.
                self.regs.borrow_mut().insert(self.sec2_base + FALCON_DMACTL, 0);
            }
        }
    }

    fn build_synthetic_pascal_vbios(include_fwsec: bool) -> Vec<u8> {
        let img0_len = 1024usize;
        let total = if include_fwsec { 2048 } else { 1536 };
        let mut rom = vec![0u8; total];

        // Image 0: PCI AT x86 (0x00) containing BIT header + DEVINIT(0x49) + FALCON_DATA(0x70)
        rom[0..2].copy_from_slice(&0xaa55u16.to_le_bytes());
        rom[0x18..0x1a].copy_from_slice(&0x0040u16.to_le_bytes());
        rom[0x40..0x44].copy_from_slice(b"PCIR");
        rom[0x44..0x46].copy_from_slice(&0x10deu16.to_le_bytes());
        rom[0x46..0x48].copy_from_slice(&0x1bb3u16.to_le_bytes());
        rom[0x50..0x52].copy_from_slice(&((img0_len / 512) as u16).to_le_bytes());
        rom[0x54] = 0x00; // PCI-AT x86
        rom[0x55] = 0x00; // more images follow

        // BIT header at 0x100
        let bit = 0x100usize;
        rom[bit..bit + 6].copy_from_slice(b"\xff\xb8BIT\0");
        rom[bit + 8] = 12; // header size
        rom[bit + 9] = 6; // token size
        rom[bit + 10] = 2; // 2 tokens
        // Token 0: DEVINIT (0x49)
        rom[bit + 12] = 0x49;
        rom[bit + 13] = 2;
        rom[bit + 14..bit + 16].copy_from_slice(&16u16.to_le_bytes());
        rom[bit + 16..bit + 18].copy_from_slice(&0x0200u16.to_le_bytes());
        // Token 1: FALCON_DATA (0x70)
        rom[bit + 18] = 0x70;
        rom[bit + 19] = 1;
        rom[bit + 20..bit + 22].copy_from_slice(&8u16.to_le_bytes());
        rom[bit + 22..bit + 24].copy_from_slice(&0x0280u16.to_le_bytes());

        // Image 1: EFI (0x03) or FWSEC (0xe0)
        let img1_off = img0_len;
        let img1_blocks = ((total - img1_off) / 512) as u16;
        rom[img1_off..img1_off + 2].copy_from_slice(&0xaa55u16.to_le_bytes());
        rom[img1_off + 0x18..img1_off + 0x1a].copy_from_slice(&0x0020u16.to_le_bytes());
        let pcir1 = img1_off + 0x20;
        rom[pcir1..pcir1 + 4].copy_from_slice(b"PCIR");
        rom[pcir1 + 0x04..pcir1 + 0x06].copy_from_slice(&0x10deu16.to_le_bytes());
        rom[pcir1 + 0x06..pcir1 + 0x08].copy_from_slice(&0x1bb3u16.to_le_bytes());
        rom[pcir1 + 0x10..pcir1 + 0x12].copy_from_slice(&img1_blocks.to_le_bytes());
        rom[pcir1 + 0x14] = if include_fwsec { 0xe0 } else { 0x03 };
        rom[pcir1 + 0x15] = 0x80; // last image

        rom
    }

    #[test]
    fn test_gp104_spec_and_falcon_bases() {
        let bar0 = MockBar0::new_gp104();
        let spec = Spec::probe(&bar0).unwrap();
        assert_eq!(spec.chipset, Chipset::GP104);
        assert_eq!(spec.arch, Architecture::Pascal);
        assert_eq!(spec.rm_tag, Some(0x1f));
        assert_eq!(spec.boot_path, BootPath::PascalAcrHostRm);
        assert_eq!(FalconEngine::Sec2.bar0_base(spec.arch), 0x0008_7000);
        assert_eq!(FalconEngine::Sec2.bar0_base(Architecture::Turing), 0x0084_0000);
        assert_eq!(FalconEngine::Pmu.bar0_base(spec.arch), 0x0010_a000);
    }

    #[test]
    fn test_clean_room_pascal_bootstrap_prod_signature_succeeds() {
        let bar0 = MockBar0::new_gp104();
        let vbios = build_synthetic_pascal_vbios(false);
        let pmu_ucode = [0x90u8; 512];
        let pmu_ls_sig = [0xabu8; 192];
        let mut dmem = vec![0u32; 32];
        let sig_patch = HsSignaturePatch {
            patch_loc: 0x20,
            prod_sig: [0x1111_2222, 0x3333_4444, 0x5555_6666, 0x7777_8888],
            dbg_sig: [0xaaaa_bbbb, 0xcccc_dddd, 0xeeee_ffff, 0x0000_9999],
        };
        let imem = [0x0102_0304u32; 70]; // spans 2x 256B IMEM blocks cleanly
        let ls_images = [LsImageSpec {
            falcon_id: LsFalconId::Pmu,
            ucode_image: &pmu_ucode,
            ls_signature: &pmu_ls_sig,
            bootloader_size: 24,
            app_code_offset: 0,
            app_code_size: 256,
            app_data_offset: 256,
            app_data_size: 256,
        }];

        let report = run_pascal_bootstrap(
            &bar0,
            PascalBootPlan {
                sysmem_flush_dma_addr: 0x8000_0000,
                vbios_rom: &vbios,
                fuse: FuseStatus {
                    priv_sec_enabled: true,
                    mode: FuseMode::Production,
                },
                wpr1_size: 2 * 1024 * 1024,
                top_reserved: 256 * 1024,
                acr_hs_imem: &imem,
                acr_hs_dmem: &mut dmem,
                acr_hs_sig: sig_patch,
                acr_boot_vec: 0x0000_0100,
                ls_images: &ls_images,
                max_polls: 16,
            },
        )
        .unwrap();

        assert_eq!(report.stage, BootStage::LsFalconsBootstrapped);
        assert_eq!(report.fb_layout.vidmem_bytes, 8 * 1024 * 1024 * 1024);
        assert_eq!(bar0.read32(NV_PFB_NISO_FLUSH_SYSMEM_ADDR), 0x8000_0000 >> 8);
        assert_eq!(bar0.read32(bar0.sec2_base + FALCON_BOOTVEC), 0x100);
        assert_eq!(bar0.imem_words.borrow().len(), imem.len());
        assert_eq!(&bar0.dmem_words.borrow()[8..12], &sig_patch.prod_sig);
        assert!(report.vbios.has_efi_image);
        assert!(report.vbios.falcon_data_token.is_some());
    }

    #[test]
    fn test_silicon_signature_failure_and_privsec_gates() {
        let mut bar0 = MockBar0::new_gp104();
        let vbios = build_synthetic_pascal_vbios(false);
        let pmu_ucode = [0x90u8; 256];
        let pmu_ls_sig = [0xabu8; 192];
        let sig_patch = HsSignaturePatch {
            patch_loc: 0x10,
            prod_sig: [1, 2, 3, 4],
            dbg_sig: [5, 6, 7, 8],
        };
        let ls_images = [LsImageSpec {
            falcon_id: LsFalconId::Pmu,
            ucode_image: &pmu_ucode,
            ls_signature: &pmu_ls_sig,
            bootloader_size: 24,
            app_code_offset: 0,
            app_code_size: 128,
            app_data_offset: 128,
            app_data_size: 128,
        }];

        // Case 1: PrivSec disabled must abort before launching Falcon HS.
        let mut dmem = vec![0u32; 16];
        let err = run_pascal_bootstrap(
            &bar0,
            PascalBootPlan {
                sysmem_flush_dma_addr: 0x1000_0000,
                vbios_rom: &vbios,
                fuse: FuseStatus {
                    priv_sec_enabled: false,
                    mode: FuseMode::Debug,
                },
                wpr1_size: 1024 * 1024,
                top_reserved: 0,
                acr_hs_imem: &[0x11u32; 4],
                acr_hs_dmem: &mut dmem,
                acr_hs_sig: sig_patch,
                acr_boot_vec: 0,
                ls_images: &ls_images,
                max_polls: 8,
            },
        )
        .unwrap_err();
        assert_eq!(err, NvError::PrivSecDisabled);

        // Case 2: Hardware BROM/HS returns MAILBOX0 = 0x4f on signature rejection.
        bar0.mailbox0_on_start = 0x4f;
        let err_brom = run_pascal_bootstrap(
            &bar0,
            PascalBootPlan {
                sysmem_flush_dma_addr: 0x1000_0000,
                vbios_rom: &vbios,
                fuse: FuseStatus {
                    priv_sec_enabled: true,
                    mode: FuseMode::Debug,
                },
                wpr1_size: 1024 * 1024,
                top_reserved: 0,
                acr_hs_imem: &[0x11u32; 4],
                acr_hs_dmem: &mut dmem,
                acr_hs_sig: sig_patch,
                acr_boot_vec: 0,
                ls_images: &ls_images,
                max_polls: 8,
            },
        )
        .unwrap_err();
        assert_eq!(err_brom, NvError::FalconErrorCode(0x4f));
    }

    #[test]
    fn test_turing_chipset_routed_away_from_pascal_path() {
        let bar0 = MockBar0::new_gp104();
        // Override BOOT_42 to TU102 (arch=0x16, impl=0x2)
        bar0.write32(NV_PMC_BOOT_42, (0x16 << 24) | (0x2 << 20));
        let vbios = build_synthetic_pascal_vbios(true);
        let mut dmem = vec![0u32; 16];
        let err = run_pascal_bootstrap(
            &bar0,
            PascalBootPlan {
                sysmem_flush_dma_addr: 0x1000_0000,
                vbios_rom: &vbios,
                fuse: FuseStatus {
                    priv_sec_enabled: true,
                    mode: FuseMode::Production,
                },
                wpr1_size: 1024 * 1024,
                top_reserved: 0,
                acr_hs_imem: &[0u32; 4],
                acr_hs_dmem: &mut dmem,
                acr_hs_sig: HsSignaturePatch {
                    patch_loc: 0,
                    prod_sig: [0; 4],
                    dbg_sig: [0; 4],
                },
                acr_boot_vec: 0,
                ls_images: &[],
                max_polls: 4,
            },
        )
        .unwrap_err();
        assert_eq!(err, NvError::UnsupportedChipset(Chipset::TU102 as u16));
    }

    #[test]
    fn test_dbg_signature_patch_and_alignment_guards() {
        let fuse_dbg = FuseStatus {
            priv_sec_enabled: true,
            mode: FuseMode::Debug,
        };
        let patch = HsSignaturePatch {
            patch_loc: 0x08,
            prod_sig: [1, 2, 3, 4],
            dbg_sig: [0xdead_beef, 0xcafe_babe, 0xface_feed, 0x1234_5678],
        };
        let mut dmem = vec![0u32; 8];
        crate::acr::apply_hs_signature_patch(&mut dmem, fuse_dbg, &patch).unwrap();
        assert_eq!(&dmem[2..6], &patch.dbg_sig);

        let bad_align = HsSignaturePatch {
            patch_loc: 0x05,
            ..patch
        };
        assert_eq!(
            crate::acr::apply_hs_signature_patch(&mut dmem, fuse_dbg, &bad_align).unwrap_err(),
            NvError::InvalidAlignment
        );
    }

    #[test]
    fn test_fb_wpr1_and_invalid_vbios_rejected() {
        let bar0 = MockBar0::new_gp104();
        let err_align = crate::fb::FbLayout::plan_wpr1(&bar0, 4096, 0).unwrap_err();
        assert_eq!(err_align, NvError::InvalidAlignment);

        assert_eq!(
            crate::fb::FbLayout::vidmem_size_from_reg(0).unwrap_err(),
            NvError::OutOfBounds
        );

        let bad_rom = [0x00u8; 512];
        assert_eq!(
            crate::vbios::parse_vbios(&bad_rom).unwrap_err(),
            NvError::InvalidRomHeader
        );
    }

    #[test]
    fn test_live_silicon_fuses_nvenc_and_real_descriptor_parsers() {
        let bar0 = MockBar0::new_gp104();
        // Observed live Tesla P4 silicon fuse & PRI privilege mask values
        bar0.write32(crate::regs::NV_FUSE_PRIV_SEC_STATUS, 0x0000_0001);
        bar0.write32(crate::regs::NV_PFB_PRI_MMU_WPR_PRIV_BASE, 0xbadf_5040);
        let fuse = FuseStatus::probe(&bar0, FuseMode::Production);
        assert!(fuse.priv_sec_enabled);
        assert!(crate::regs::is_pri_error(bar0.read32(crate::regs::NV_PFB_PRI_MMU_WPR_PRIV_BASE)));

        let nvenc0 = crate::falcon::FalconDevice::new(&bar0, FalconEngine::Nvenc0, Architecture::Pascal);
        assert_eq!(nvenc0.base(), 0x001c_8000);
        assert!(!nvenc0.is_ls_mode());
        bar0.write32(nvenc0.base() + crate::falcon::FALCON_SCTL, crate::falcon::SCTL_LS_ACTIVE);
        assert!(nvenc0.is_ls_mode());

        // Real 24B FECS BL descriptor header extracted from 580.178.04 (`0x2bd98`, bootvec=0x5e00)
        let fecs_bl_bytes: [u8; 24] = [
            0x5e, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
            0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
        ];
        let fecs_bl = crate::firmware::FlcnBlDesc::parse(&fecs_bl_bytes).unwrap();
        assert_eq!(fecs_bl.boot_vec, 0x5e00);
        assert_eq!(fecs_bl.data_off, 0x100);

        // Real 192B LS signature XOR footer check for PMU (fid=0) & NVENC (fid=4)
        for (expected, word44) in [(LsFalconId::Pmu, 0xe931_c2b6u32), (LsFalconId::Nvenc, 0xe931_c2b2u32)] {
            let mut lsf = [0u8; 192];
            lsf[176..180].copy_from_slice(&word44.to_le_bytes());
            lsf[180..184].copy_from_slice(&0xd777_b203u32.to_le_bytes());
            let parsed = crate::firmware::LsfSigInfo::parse(&lsf).unwrap();
            assert_eq!(parsed.falcon_id, expected);
        }

        // Real 2-section packed GR netlist TOC check
        let mut net = vec![0u8; 64];
        net[0..4].copy_from_slice(&0u32.to_le_bytes());
        net[4..8].copy_from_slice(&2u32.to_le_bytes());
        net[8..12].copy_from_slice(&1u32.to_le_bytes());
        net[12..16].copy_from_slice(&16u32.to_le_bytes());
        net[16..20].copy_from_slice(&32u32.to_le_bytes());
        net[20..24].copy_from_slice(&0u32.to_le_bytes());
        net[24..28].copy_from_slice(&8u32.to_le_bytes());
        net[28..32].copy_from_slice(&48u32.to_le_bytes());
        let mut secs = [crate::firmware::GrNetlistSection {
            section_type: 0,
            size: 0,
            offset: 0,
        }; 4];
        let n = crate::firmware::parse_gr_netlist_toc(&net, &mut secs).unwrap();
        assert_eq!(n, 2);
        assert_eq!(secs[0].section_type, 1);
        assert_eq!(secs[1].offset, 48);
    }

    #[test]
    fn test_real_extracted_vbios_and_prom_jffs_layouts() {
        // Test BIT 'p' EFI gap (+0x400 here) + PROM JFFS zero-alloc parsers
        let mut rom = build_synthetic_pascal_vbios(false);
        rom.resize(2048, 0);
        // Primary ends at 0x400, EFI (0x03) length is 0x200 => BIT 'p' at 0x280 points to 0x420 -> adjusted 0x620
        let raw_tbl_off = 0x0420u32;
        rom[0x280..0x284].copy_from_slice(&raw_tbl_off.to_le_bytes());
        let tbl = 0x620usize;
        rom[tbl] = 1; // version
        rom[tbl + 1] = 6; // hdr bytes
        rom[tbl + 2] = 6; // entry bytes
        rom[tbl + 3] = 1; // count
        rom[tbl + 6] = 0x04; // Devinit app_id
        rom[tbl + 7] = 0x01; // PMU target_id
        rom[tbl + 8..tbl + 12].copy_from_slice(&0x0480u32.to_le_bytes()); // adjusted => 0x680
        rom[0x680..0x682].copy_from_slice(&0x0201u16.to_le_bytes());

        let mut apps = [crate::vbios::VbiosFalconAppEntry {
            app_id: 0,
            target_id: 0,
            adjusted_rom_offset: 0,
            header_tag: 0,
        }; 4];
        let (summary, count) = crate::vbios::parse_vbios_falcon_apps(&rom, &mut apps).unwrap();
        assert_eq!(summary.efi_gap_bytes, 512);
        assert_eq!(count, 1);
        assert_eq!(apps[0].app_id, 4);
        assert_eq!(apps[0].adjusted_rom_offset, 0x680);
        assert_eq!(apps[0].header_tag, 0x0201);

        let mut prom = vec![0xffu8; 512];
        prom[0..12].copy_from_slice(b"JFFS\x02\x00\x00\x00\x00\x02\x00\x00");
        prom[0x80..0x84].copy_from_slice(b"\x02ROM");
        prom[0x100..0x104].copy_from_slice(b"\x06ECC");
        let mut tags = [[0u8; 3]; 8];
        let jffs = crate::vbios::parse_prom_inforom(&prom, &mut tags).unwrap();
        assert_eq!(jffs.object_count, 2);
        assert_eq!(&tags[0], b"ROM");
        assert_eq!(&tags[1], b"ECC");
    }

    #[test]
    fn test_real_acr_hs_and_rtos_headers() {
        // Real 36B ACR_LOAD descriptor (`0x032668` in 580.178.04)
        let mut acr = [0u8; 36];
        let acr_words: [u32; 9] = [0, 0x100, 0x2b00, 0x1000, 1, 0x100, 0x2a00, 0x100, 0];
        for (i, w) in acr_words.iter().enumerate() {
            acr[i * 4..(i + 1) * 4].copy_from_slice(&w.to_le_bytes());
        }
        let parsed_acr = crate::firmware::AcrHsDesc::parse(&acr).unwrap();
        assert_eq!(parsed_acr.imem_size, 11008);
        assert_eq!(parsed_acr.dmem_size, 4096);
        assert_eq!(parsed_acr.app_code_size, 10752);

        // Real PMU RTOS v4 header words (`flags=4`, app_entry=0x5e00, total=300288B)
        let mut pmu = [0u8; 0x90];
        pmu[0..4].copy_from_slice(&0x290u32.to_le_bytes());
        pmu[8..12].copy_from_slice(&4u32.to_le_bytes());
        pmu[0x5c..0x60].copy_from_slice(&0x5e00u32.to_le_bytes());
        pmu[0x60..0x64].copy_from_slice(&0x5e00u32.to_le_bytes());
        pmu[0x64..0x68].copy_from_slice(&0x100u32.to_le_bytes());
        pmu[0x68..0x6c].copy_from_slice(&0x49400u32.to_le_bytes());
        pmu[0x88..0x8c].copy_from_slice(&99u32.to_le_bytes());
        let parsed_pmu = crate::firmware::RtosDesc::parse(&pmu).unwrap();
        assert_eq!(parsed_pmu.app_entry, 0x5e00);
        assert_eq!(parsed_pmu.total_expected_image_bytes, 300_288);
        assert_eq!(parsed_pmu.num_overlays, 99);

        // Real SEC2 RTOS v2 header words (`flags=2`, app_entry=0xfd00, total=221440B)
        let mut sec2 = [0u8; 0x90];
        sec2[0..4].copy_from_slice(&0x290u32.to_le_bytes());
        sec2[8..12].copy_from_slice(&2u32.to_le_bytes());
        sec2[0x58..0x5c].copy_from_slice(&0xfd00u32.to_le_bytes());
        sec2[0x5c..0x60].copy_from_slice(&0xfd00u32.to_le_bytes());
        sec2[0x60..0x64].copy_from_slice(&0x200u32.to_le_bytes());
        sec2[0x64..0x68].copy_from_slice(&0x35f00u32.to_le_bytes());
        sec2[0x88..0x8c].copy_from_slice(&14u32.to_le_bytes());
        let parsed_sec2 = crate::firmware::RtosDesc::parse(&sec2).unwrap();
        assert_eq!(parsed_sec2.app_entry, 0xfd00);
        assert_eq!(parsed_sec2.total_expected_image_bytes, 221_440);
        assert_eq!(parsed_sec2.num_overlays, 14);
    }
}
