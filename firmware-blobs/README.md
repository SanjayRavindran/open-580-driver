# Isolated GP104 Firmware Blobs (`firmware-blobs/`)

> [!CAUTION]
> **Strict License & Binary Isolation Boundary:**
> This directory holds ONLY NVIDIA proprietary / hardware-signed binary images and VBIOS/PROM dumps extracted from **NVIDIA R580 (`580.178.04`)** and live **Tesla P4 (`GP104GL`, PCI `10de:1bb3`)** silicon.
> All clean-room source code lives outside this directory in [`nova-pascal/`](../nova-pascal/), [`tools/`](../tools/), [`kmods/`](../kmods/), and [`open-r580-driver/`](../open-r580-driver/).

## Why These Blobs Are Required (Silicon Proof)
On fused retail Tesla P4 hardware (`NV_FUSE[0x21400] = 0x00000001`, `PrivSec` enabled), Level-0 host CPU access to `WPR` MMU control registers returns `0xbadf5040`, and all Falcon engines stay in untrusted (`sctl = 0x3000`) / halted state without valid NVIDIA HS/LS signatures. Neither `PMU`, `SEC2`, `NVENC`, `FECS`, nor `GPCCS` can lock `WPR1` or authenticate microcode in `sctl = 0x7021` (LS Mode) from unsigned open-source host software alone.

## Directory Layout (`gp104/`)

| Folder | Contents | Provenance (`580.178.04` / Silicon) |
| :--- | :--- | :--- |
| `acr/` | 18 GP104 ACR bindata archives (`acr_load`, `acr_unload`, `acr_bsi_lock`, `acr_bl`, and auxiliary HS stages/signatures) | `_nv005947rm` (`HAL_IMPL = 0x1f`) & `_nv019405rm` |
| `pmu/` | `gp104_pmu_rtos` (`300,288`B + `940`B desc), `gp104_pmu_bl` (`768`B + `24`B), `gp104_pmu_ls_sig.bin` (`192`B, `falcon_id = 0`) | `_nv006035rm` (`HAL_IMPL = 0x1f`, `_nv052098rm`/`_nv052066rm`/`_nv052124rm`) |
| `sec2/` | `gp104_sec2_rtos` (`221,440`B + `656`B + `192`B `fid = 7`), `gp104_sec2_hs` (`65,792`B + `204`B), `gp104_sec2_bl` (`768`B + `24`B) | `_nv006010rm` (`HAL_IMPL = 0x1f`, `_nv055720rm`/`_nv055671rm`/`_nv055663rm`) |
| `nvenc/` | `gp104_nvenc0_ucode.bin` & `nvenc1` (`230,144`B each + `364`B descs), `gp104_nvenc_hs_scrub`, `gp104_nvenc_ls_sig.bin` (`192`B, `falcon_id = 4`) | `_nv005951rm` (`HAL_IMPL = 0x1f`, `_nv018314rm`/`_nv018350rm`) |
| `nvdec/` | `gp104_nvdec_ucode.bin` (`110,336`B), `gp104_nvdec_desc.bin` (`236`B) | `_nv006002rm` (`HAL_IMPL = 0x1f`, `_nv046336rm`/`_nv046326rm`) |
| `gr/` | `FECS` & `GPCCS` `192`B LS sigs (`fid = 2, 3`), `512`B + `24`B BLs, `gp104_gr_netlist.bin` (`90,552`B, `Jun 28 2019`), and unpacked `netlist_sections/` (`32` sections including plaintext `FECS`/`GPCCS` IMEM+DMEM & SW CTX) | `_nv005875rm`, `_nv005879rm`, `_nv005889rm` (`HAL_IMPL = 0x1f`) |
| `vbios/` | Live GPU `gp104_vbios.rom` (`239,104`B), raw `NV_PROM` `prom_raw.bin` (`262,144`B incl. `JFFS` InfoROM), and 3 BIT `'p'` Falcon apps (`01`, `04` Devinit, `05` HULK) | Live Tesla P4 hardware (`NV_PROM` `0x300000` & `nouveau` debugfs) |
| `BLOB_MANIFEST.json` / `manifest.json` | Machine-readable SHA-256 checksums, offsets, LS `falcon_id` decodings, VBIOS offsets, and PROM JFFS records | Generated cleanly by [`nv_bindata_analyzer.py`](../../tools/nv_bindata_analyzer.py) & [`nv_fw_descriptor_parser.py`](../../tools/nv_fw_descriptor_parser.py) |

