# 02 — Reverse Engineering of NVIDIA R580.178.04 (`nv-kernel.o_binary`)

## 1. Methodology & `rea@6.8.0` + Ghidra `12.1.4` Workflow

The driver package `NVIDIA-Linux-x86_64-580.178.04.run` was extracted without modifying the running system kernel driver. Its precompiled proprietary RM object (`nv-kernel.o_binary`, `112,677,456` bytes, SHA-256 `5269a23ffba19a17fe16f076463e7ad4151254659441d1711bee6c6221ff6f4b`) and open kernel C shim (`kernel/`) were analyzed using:

- `rea@6.8.0` byte-entropy analysis (`ev_e98d0d41075513136158f701c71e6c46925fa25aa3199e34d46d077592164450`)
- Four focused ELF relocation slices decompiled via Ghidra `12.1.4` (`analysis_profile_digest = ae80ece8c83379eddabbdf379bbffca5da84c7df7fa7c159f45fa810468f85d9`) with `0` failed functions:
  - `p4_focus.o`: Falcon loader / ACR / PMU routines (`73` functions written, `0` failed; target SHA-256 `23a0d453...`)
  - `p4_sig_focus.o`: Signature / fuse / RPC routines (`54` functions written, `0` failed; target SHA-256 `418ab4bc...`)
  - `p4_hal_chip.o`: Chip & IP HAL wiring (`ev_bdff3df0a14308985f9d0f5d02e2b3d569bfc59ba7bd1766fd6ec442afd763ed`, `25` functions written, `0` failed)
  - `p4_hal_chip2.o`: Physical vs. Kernel RM bindata registration (`ev_8bbe39dee99e97b534bcb309dc3cd5524d6b2510f18fac8a3afc42007253e92d`, `11` functions written, `0` failed)
  - `p4_gr_hal.o`: GR `FECS` (`_nv005875rm`), `GPCCS` (`_nv005879rm`), and packed GR Netlist (`_nv005889rm`) HALs (`ev_f81035f1fba65309ad112c8efd10afc44aacfab94858f43942a449194a949fa0`, `638` functions written, `0` failed)
- `rea inspect-native-exports` on `libnvidia-encode.so.580.178.04` (`ev_59611b79e3f862d563d76410e43ccdb87848ab385cbf9fda9ab942dbe56f1cbf`), confirming `NvEncodeAPICreateInstance` and `NvEncodeAPIGetMaxSupportedVersion` entrypoints (`ptrs[30]` / `+0xf0` = `nvEncOpenEncodeSessionEx` for CUDA contexts, `ptrs[2]` / `+0x10` = `nvEncGetEncodeGUIDCount`).

*(Tooling note recorded accurately: `rea` `inspect_binary_layout` returned `provider_unavailable` because `REA_PWNTOOLS_PYTHON` was unset in that session, and `inspect_kernel_module` rejected `nvidia.ko` at `136,222,376` bytes against the `67,108,864`-byte file budget; `.rodata` structural inspection was completed via ELF64 `.rela.rodata` / `.symtab` scanning in [nv_bindata_analyzer.py](../tools/nv_bindata_analyzer.py).)*

## 2. Why `nv-kernel.o_binary` Appears High-Entropy (`7.4854` bits/byte)

`nv_kernel_entropy.json` measures overall Shannon entropy across `27,510` 4 KiB blocks:
- File entropy: `7.4854` (threshold `high >= 7.2`)
- High-entropy blocks (`>= 7.2`): `19,423` blocks (`70.6%`), including a single contiguous high-entropy run of `24,514,560` bytes starting at offset `28,102,656`.
- Low-entropy blocks (`<= 1.0`): `302` blocks.

Structural `.rodata` analysis proves **this entropy is standard compressed firmware data, not x86 code packing or runtime obfuscation**:
1. `.rodata` is `82,529,056` bytes (`~73.2%` of the entire `nv-kernel.o_binary` file) and contains `63,482` ELF relocations (`.rela.rodata`).
2. `nv-kernel.o_binary` strips human C symbol names down to sequential `_nvXXXXXXrm` identifiers, leaving only lightweight ABI stubs (`rm_is_supported_device`, `os_*`) readable via `nm`.
3. Compressed bindata payloads use **headerless raw DEFLATE (`zlib.decompress(..., -15)`)** implemented inside RM helpers `_nv059713rm`, `_nv059715rm`, and `_nv059716rm` using a `0x7fff` sliding window (`_nv017871rm`..`_nv017881rm` dispatch chunk/entry reads).

## 3. Chip Identification & Internal RM Dispatch Mapping (`GP104 = Tag 0x1f`)

Decompiling `_nv034616rm` (`0xa4d0`), `rm_is_supported_device` (`0xd67470`), `_nv006052rm` (`0xbf2230`), and `_nv006053rm` (`0xbf2680`) reveals how R580 classifies PCI/MMIO hardware into internal HAL bitmasks:

1. **Silicon probing**: RM reads `NV_PMC_BOOT_0` (`0x0000_0000`) and `NV_PMC_BOOT_42` (`0x0000_0a00`) to extract `arch` and `impl`, matching them against `_nv019377rm` (`113` device entries x `12` bytes).
2. **`_nv006052rm` (`0xbf2230`) chip enum mapper**: Translates `(arch, impl)` into an internal RM HAL bit index (`bucket = tag >> 5`, `mask = 1 << (tag & 31)`):
   - `GP100` (`arch=0x13, impl=0x0`) -> `tag 0x1d` (`29`)
   - `GP102` (`arch=0x13, impl=0x2`) -> `tag 0x1e` (`30`)
   - **`GP104` (`arch=0x13, impl=0x4`) -> `tag 0x1f` (`31`, bucket `0`, bit `31`)**
   - `GP106` (`arch=0x13, impl=0x6`) -> `tag 0x20` (`32`)
   - `GP107` (`arch=0x13, impl=0x7`) -> `tag 0x21` (`33`)
   - `GP108` (`arch=0x13, impl=0x8`) -> `tag 0x22` (`34`)
   - `TU102` (`arch=0x16, impl=0x2`) -> `tag 0x25` (`37`)

## 4. `BindataStorage` Archive & Entry Binary Layout

Reverse-engineering `_nv005947rm`, `_nv005956rm`, `_nv006010rm`, and `_nv006035rm` decoded the exact `.rodata` firmware container structs (implemented cleanly in [nv_bindata_analyzer.py](../tools/nv_bindata_analyzer.py)):

```text
BindataStorage (16-byte header + N x 16-byte records in .rodata):
  +0x00  u32 count
  +0x04  u32 pad (0)
  +0x08  struct { u32 bindata_id; u32 pad; u64 *entry_ptr; } items[count]

BindataEntry (24 bytes, struct "<IIQII"):
  +0x00  u32 uncompressed_size
  +0x04  u32 compressed_size
  +0x08  u64 *payload_ptr       (relocated via R_X86_64_64 in .rela.rodata)
  +0x10  u32 flags              (bit 0 = 1 => raw DEFLATE stream, wbits=-15)
  +0x14  u32 cumulative_offset  (0 for uncompressed)
```

Running `nv_bindata_analyzer.py` on the live `580.178.04` `nv-kernel.o_binary` verified:
- `.rodata` size: `82,529,056` bytes (`63,482` `.rela.rodata` relocations)
- Valid `BindataStorage` archives discovered: **`471`**
- Compressed entries decoded: **`773`**
- Total uncompressed payload extracted across all architectures: **`67,799,225` bytes**

## 5. Complete GP104 (`Tag 0x1f`) Firmware Catalog inside R580.178.04

A critical finding in `p4_hal_chip2.o` explains why Turing+ open drivers miss GP104 assets: **Kernel RM HALs** (`KernelAcr` `_nv006003rm`, `KernelSec2` `_nv006002rm`, `KernelPmu` `_nv005951rm`, `KernelNvdec` `_nv006005rm`) only bind Turing+ GSP-client tables (`tag >= 0x24`), while **Physical RM HALs** (`_nv005947rm`, `_nv005956rm`, `_nv006010rm`, `_nv006035rm`) carry all signed GP104 blobs:

| Archive / Blob | `.rodata` Offset | `bindata_id` | Size (B) | Observed SHA-256 | Role / Strings / Notes |
| :--- | :--- | :--- | ---: | :--- | :--- |
| `gp104_acr_bl` | `0x29068` | `0x23` | 1,280 | `02df5be808af2ece6272695c1050037113c502bab60a5e51e211342c7e3ce645` | HS ACR bootloader image |
| `gp104_acr_bl` | `0x29068` | `0x18` | 36 | `c5dd2255d82eb3ac5790f71c852dc327c961829c58e956027d50ab20e66ccb99` | ACR BL header |
| `gp104_acr_load` | `0x25af0` | `0x23` | 17,152 | `8e5a8fcce81f04947b20d86a7a0071e99c7687ca04f734a110e31a98c8c99fae` | HS ACR load Falcon image |
| `gp104_acr_load` | `0x25af0` | `0x18` | 36 | `910c2d7e54eb756119b14bba4d8df08a7e961540a92988824ad8669264da3865` | ACR load header |
| `gp104_acr_load` | `0x25af0` | `0x3d` | 16 | `600638cf0bb29349d07ec773c06f0ee23e46a13c534f4375d0241944b9464870` | **PROD HS signature** |
| `gp104_acr_load` | `0x25af0` | `0x39` | 16 | `ca58ad2a46730c4c1bb37b3dcb435608258c2f29ee78bf5fd70c44c0dcdd8030` | **DBG HS signature** |
| `gp104_acr_load` | `0x25af0` | `0x3b` | 4 | `da9d1ddace61f318ab9d1f034cfedbb61f700f7f693a8370f06c8efe969db576` | `patch_loc = 0x3a20` (`20 3a 00 00` LE) |
| `gp104_acr_load` | `0x25af0` | `0x3a` | 4 | `df3f619804a92fdb4057192dc43dd748ea778adc52bc498ce80524c014b81119` | `patch_sig` (`00 00 00 00`) |
| `gp104_acr_unload` | `0x231d0` | `0x23` | 3,584 | `06ee5ea80dacb84ff15c5551801acb27c4d62615304f5db598f591078915a648` | HS ACR unload Falcon image |
| `gp104_acr_unload` | `0x231d0` | `0x18` | 36 | `621753dfbd42dddc784489f965be7849cff862ad4050826305cf7526cc9b7cef` | ACR unload header |
| `gp104_pmu_bl` | `0x25c80` | `0x23` | 768 | `18328b687f89e21857dddffc7ed8d41b6554367b95660af0b158e9dd3bba8293` | PMU bootloader |
| `gp104_pmu_bl` | `0x25c80` | `0x12` | 24 | `dbae064d7b4d30f8575eb51a00ad577776306c3cfb0170a6634febf6a547b7bc` | PMU BL descriptor |
| `gp104_pmu_rtos` | `0x26108` | `0x23` | 300,288 | `2980c0eea45a0bfcb93a5138b8ab222826e8248672672caef2a88b6eb0a45b5f` | **GP104 PMU RTOS image** (`bootvec=0x5e00`) |
| `gp104_pmu_rtos` | `0x26108` | `0x12` | 940 | `02f2af6553e42fa4cdb07613799cff5a3abd62056bf59685806f910e68b6733a` | PMU descriptor (`Tue Jun 23 10:23:1 2026`, `99` overlays) |
| `gp104_pmu_ls_sig` | `0x2dac0` | single | 192 | `5f61eef8f1749300e248e7d5aa196d7abfc106d38ee00f3a4a924c9cb32211b5` | **GP104 PMU LS sig (`_nv052124rm`, `fid=0`)** |
| `gp104_sec2_rtos` | `0x22e68` | `0x23` | 221,440 | `7b30debec1182643fd91aa003e8aa3b026a00058e22e3c6f9ada86ed8990868b` | SEC2 RTOS (`bootvec=0xfd00`) |
| `gp104_sec2_rtos` | `0x22e68` | `0x12` | 656 | `671429aebb50b2e4b01e8677528b4db1a86edaa3d8a5af0a74b7ab1af5d75d30` | SEC2 desc (`Fri Apr 5 1:32:57 2019`, `14` overlays) |
| `gp104_sec2_rtos` | `0x22e68` | `0x27` | 192 | `8403cfdb08fdc589fa671a12483709803a74fac25470decdd9667c42f6de35a8` | SEC2 LS sig (`fid=7`) |
| `gp104_sec2_hs` | `0x22fd8` | `0x23` | 65,792 | `0208fc410b6d95d9b5bb84ec3360c350e8c5a35084eb3bc574b3275241297bc7` | SEC2 HS sub-ucode (`11` apps + `204`B desc) |
| `gp104_nvenc0_ucode` | `0x29ae8` | single | 230,144 | `5c95cf5cea36eaeb...` | NVENC0 RTOS (`21`-app `364`B desc `_nv018288rm`) |
| `gp104_nvenc_ls_sig` | `0x29f80` | single | 192 | `2ebe0dfe3cf9ea29b0968ace3e296679fda2ec219f98d8a38b42d34ac7605bcf` | **GP104 NVENC LS sig (`_nv018350rm`, `fid=4`)** |
| `gp104_nvdec_ucode` | `0x2d640` | single | 110,336 | `cd91dce1fe5ceb5c...` | NVDEC ucode (`13`-app `236`B desc `_nv046326rm`) |
| `gp104_fecs_ls_sig` | `0x2b660` | single | 192 | `bdb7d23cd0d85354377eb65d7343549e83df23c71beb0b0bdcfb77082bfe9a16` | FECS LS sig (`fid=2`) + `512`B BL (`bootvec=0x5e00`) |
| `gp104_gpccs_ls_sig` | `0x2c800` | single | 192 | `452d32671ff1b40e68e1600db23875b11335c55fda7808e4d77f8f70eda5d356` | GPCCS LS sig (`fid=3`) + `512`B BL (`bootvec=0x3400`) |
| `gp104_gr_netlist` | `0x2ae98` | single | 90,552 | `0cfca0632f33f37d...` | Packed 32-section GR Netlist (`_nv032828rm`, `Jun 28 2019`) |

Note on `_nv029066rm` `LsFalconId` footer semantics: executing every decompiled Physical HAL initializer against `HAL_IMPL = 0x1f` and unmasking `[0xb0..0xc0]` of the `192`-byte LSF wrappers with `0xe931c2b6` proved `.rodata+0x29f80` (`_nv018350rm`) carries `falcon_id = 4` (`NVENC`), whereas `.rodata+0x2dac0` (`_nv052124rm`) carries `falcon_id = 0` (`PMU`). See [`06-FALCON-BLOBS-AND-VBIOS-INTERNALS.md`](./06-FALCON-BLOBS-AND-VBIOS-INTERNALS.md) and [`BLOB_MANIFEST.json`](../firmware-blobs/gp104/BLOB_MANIFEST.json) for all `42`+ extracted GP104 artifacts.

