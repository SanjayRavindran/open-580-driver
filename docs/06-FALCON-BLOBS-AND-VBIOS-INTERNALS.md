# 06 — Isolated Falcon Blobs, Hardware VBIOS & `NV_PROM` InfoROM Internals

All NVIDIA proprietary / signed firmware images extracted from `580.178.04` and live Tesla P4 hardware are quarantined inside [`firmware-blobs/`](../firmware-blobs/README.md), completely separated from open-source code in [`tools/`](../tools/), [`kmods/`](../kmods/), [`dkms-compat/`](../dkms-compat/), and [`open-r580-driver/`](../open-r580-driver/).

## 1. Exact Per-Engine Signed Blob Map (`HAL_IMPL = 0x1f`, GP104)

Executing every decompiled R580.178.04 Physical RM HAL initializer against `HAL_IMPL_GP104 = 0x1f` (`arch = 0x13, impl = 0x4` via `_nv006052rm`) yields the complete, ambiguity-free GP104 firmware inventory recorded in [`BLOB_MANIFEST.json`](../firmware-blobs/gp104/BLOB_MANIFEST.json):

| Subsystem | HAL Function & Slot | `.rodata` Address | Extracted Assets | Verified Details |
| :--- | :--- | :--- | :--- | :--- |
| **ACR (Access Control)** | `_nv005947rm` (`17` slots + BL `_nv019405rm`) | `0x29068`, `0x25af0`, `0x23d60`, `0x231d0`, + `14` aux slots | `acr/gp104_acr_*` (`18` archives) | `gp104_acr_load` (`17,152`B HS image, `36`B desc, `16`B PROD/DBG sigs, `patch_loc = 0x3a20`), `acr_bsi_lock` (`5,376`B), `acr_unload` (`3,584`B) |
| **PMU** | `_nv006035rm` (`+0x318`, `+0x370`, `+0x378`) | `0x26108`, `0x25c80`, **`0x2dac0`** | `pmu/gp104_pmu_rtos/`, `pmu/gp104_pmu_bl/`, `pmu/gp104_pmu_ls_sig.bin` | `300,288`B RTOS (`0x49500`, `bootvec = 0x5e00`, `99` overlays, CL `0x02498436`, build `"Tue Jun 23 10:23:1 2026"`), `768`B BL, `192`B LS sig (`fid = 0`) |
| **SEC2** | `_nv006010rm` (`+0x2c8`, `+0x2d8`, `+0x3a0`) | `0x22fd8`, `0x22e68`, `0x22b78` | `sec2/gp104_sec2_rtos/`, `sec2/gp104_sec2_hs/`, `sec2/gp104_sec2_bl/` | `221,440`B RTOS (`0x36100`, `bootvec = 0xfd00`, `14` overlays, CL `0x017e4379`, `"Fri Apr 5 1:32:57 2019"`), `192`B LS sig (`fid = 7`), `65,792`B HS + `768`B BL |
| **NVENC0 / NVENC1** | `_nv005951rm` (`+0x260`, `+0x278`, `+0x288`, `+0x2c0/c8`, `+0x2e8`) | `0x298b8`, `0x29ae8`, `0x29b00`, `0x29ab8/d0`, **`0x29f80`** | `nvenc/gp104_nvenc*` | `230,144`B x2 ucodes (`21`-app `364`B descriptors), `3,584`B HS scrub (`0x298b8`), `192`B LS sig (`fid = 4`, SHA `2ebe0dfe...`) |
| **NVDEC** | `_nv006002rm` (`+0x218`, `+0x230`) | `0x2d640`, `0x2d778` | `nvdec/gp104_nvdec_ucode.bin`, `gp104_nvdec_desc.bin` | `110,336`B ucode (`13`-app `236`B descriptor) |
| **FECS (GR Context)** | `_nv005875rm` (`+0x0c8`, `+0x130`, `+0x138`) | `0x2b660`, `0x2bc90`, `0x2bd98` | `gr/gp104_fecs_*` | `192`B LS sig (`fid = 2`), `512`B BL code, `24`B BL desc (`start_tag = 0x5e` -> `bootvec = 0x5e00`) |
| **GPCCS (GR GPC)** | `_nv005879rm` (`+0x0d8`, `+0x140`, `+0x148`, `+0x1a8`) | `0x2c800`, `0x2ce18`, `0x2cf08`, `0x2c620` | `gr/gp104_gpccs_*` | `192`B LS sig (`fid = 3`) + alt sig, `512`B BL code, `24`B BL desc (`start_tag = 0x34` -> `bootvec = 0x3400`) |
| **GR Netlist** | `_nv005889rm` (`+0x12b8` `_nv032828rm`) | `0x2ae98` | `gr/gp104_gr_netlist.bin` + `netlist_sections/` | `90,552`B container built **`Jun 28 2019`**; `32` sections unpacked (`fecs_inst` `23,253`B, `fecs_data` `2,792`B, `gpccs_inst` `13,213`B, `gpccs_data` `2,084`B, `sw_*`) |

## 2. Binary Container Structures Decoded Clean-Room

All header formats documented below are parsed deterministically without NVIDIA proprietary headers in [`nv_fw_descriptor_parser.py`](../tools/nv_fw_descriptor_parser.py) and the companion `open-vgpu-tesla-p4` GP104 HAL (`open-host-rm/open_host_rm_gp104.c`):

### 2.1 `192`-Byte `LSF_UCODE_DESC` Signature XOR Footer
Every `192`-byte LS signature entry (`PMU`, `FECS`, `GPCCS`, `NVENC`, `SEC2`) ends with `16` zero bytes (`[0xa0..0xb0]`) followed by `16` metadata bytes (`[0xb0..0xc0]`) XOR-masked with fixed little-endian words `(0xe931c2b6, 0xd777b203, 0x69a0320e, 0x62804e8f)`. Unmasking word `0` (`^ 0xe931c2b6`) recovers the exact `LsFalconId` (`0 = PMU`, `2 = FECS`, `3 = GPCCS`, `4 = NVENC`, `7 = SEC2`). This independently confirmed why `.rodata+0x29f80` (`fid = 4`) is **NVENC**, whereas `.rodata+0x2dac0` (`fid = 0`) is **PMU**.

### 2.2 RTOS (`656B` / `940B`), Bootloader (`24B`), HS (`36B`) & App (`204B` / `236B` / `364B`) Descriptors
- **Bootloader (`24B`, `6` x `u32`)**: `start_tag << 8` yields hardware `FALCON_BOOTVEC` directly (`FECS = 0x5e00`, `GPCCS = 0x3400`, `SEC2_BL = 0xfd00`).
- **HS ACR (`36B`, `9` x `u32`)**: Describes `(version, imem_off, imem_size, dmem_size, num_apps=1, app_code_off, app_code_size, app_data_off, app_data_size)`.
- **Overlay App (`204B` `SEC2_HS` `11` apps, `236B` `NVDEC` `13` apps, `364B` `NVENC` `21` apps)**: `(os_code_off, os_code_size, os_data_off, os_data_size, num_apps)`, followed by `(offset, size)` app overlay spans summing cleanly to `os_data_offset`, plus trailing OS pair.
- **RTOS (`0x290 = 656B` base + overlay pairs)**: Version/flag `<= 2` (`SEC2`, `656B`) places `app_entry = 0xfd00` and image span `0x200 + 0x35f00 = 221,440`B starting at `+0x58`; version `4` (`PMU`, `940B`) places `app_entry = 0x5e00` and span `0x100 + 0x49400 = 300,288`B at `+0x5c`, followed by `99` IMEM overlay `(offset, size)` pairs.

## 3. Live Silicon VBIOS & Hardware `NV_PROM` Extraction (`prom_raw.bin`)

Because `/sys/bus/pci/devices/0000:01:00.0/rom` is absent (`Exists: False`), we extracted firmware directly from both active `nouveau` `debugfs` (`vbios.rom`) and live GPU BAR0 `NV_PROM` (`0x300000`, `256 KiB`) via [`p4_cleanroom_diag.c`](../kmods/p4_cleanroom_diag/p4_cleanroom_diag.c):

1. **Exact VBIOS Match (`0x00000..0x3a600`)**: `prom_raw.bin`'s leading `239,104` bytes match `gp104_vbios.rom` (`SHA-256 4e3a141d5e18dbc3...`) bit-for-bit.
2. **EFI GOP (`0x03`) Gap Translation (`+0x10a00`)**:
   - `Img[0]` (`0x00000..0x0f200`, `61,952`B, `code_type = 0x00`): Legacy x86 ROM + `BIT` (`@0x1b0`, `17` tokens).
   - `Img[1]` (`0x0f200..0x1fc00`, `68,096`B = **`0x10a00`**, `code_type = 0x03`): EFI GOP + `NPDE` (`@0xf240`, `last = true`).
   - `BIT` token `0x70` (`FALCON_DATA`) and post-x86 pointers omit EFI image `Img[1]`: translating pointers `>= 0xf200` by **`+0x10a00`** resolves the `BIT 'p'` Falcon Ucode Table cleanly at **`0x1fce4`** (`ver = 1, hdr = 6, esz = 6, cnt = 5`) and extracts all 3 embedded VBIOS Falcon apps into `firmware-blobs/gp104/vbios/`:
     - `vbios_falcon_app_01_tgt_01.bin` (`17,128`B): PMU pre-OS data packing (`0x254a0`).
     - `vbios_falcon_app_04_tgt_01.bin` (`17,280`B): PMU Devinit (`0x29788`).
     - `vbios_falcon_app_05_tgt_00.bin` (`33,428`B): v2 Header (`0x3c0201`) **HULK license/reg-override** (`0x312a8`), embedding error string `ERROR: NV_UCODE_ERR_CODE_CERT_HULK_DEVID_MISMATCH`.
3. **Hardware `JFFS` InfoROM (`0x3a600..0x40000`, `prom_raw.bin`)**:
   Reading `NV_PROM` directly captured the `23,040` bytes past `vbios.rom`, revealing the `JFFS` v2 filesystem (`version = 2`, `fs_size = 0x8000`) laid out in `128`-byte (`0x80`) blocks with RO (`0x02`) and RW (`0x06`) records:
   - `ROM` (root directory), `IMG` (`G414.0200.00.03`, `PCI 10de:1bb3`, subsys `10de:11d8`), `OBD` (`"Tesla P4"`, date `2018-06-16`, serial `0322418175550`, SKU `900-2G414-0000-001`, `S161-0172-600`, `A01P699-2G414-0200-110`), `OEM`, `ECC` (`4,633`B), `PBL` (`1,484`B), and `BBX` (`8,125`B).

