# 01 — Hardware, OS & Open Firmware Inventory

## 1. Host Environment

| Property | Observed Value |
| :--- | :--- |
| Host IP / Board / Firmware | `192.168.1.24` (`br0`), Gigabyte `H61M-S1 PLUS`, **UEFI Coreboot (`df45ea2c49b4-dirty`)** with Intel VT-d (`DMAR` / `x2apic` / 64-bit Above-4G BARs) |
| CPU & Memory | Intel Xeon E3-1275 V2 @ 3.50 GHz (4 cores / 8 threads, `x86-64-v2`), 14 GiB DDR3 RAM |
| Distribution | Ubuntu 26.04.1 LTS (Resolute Raccoon) |
| Kernel Release | `7.0.0-43-generic` (`x86_64`) |
| Active Driver Stacks | **`open-r580-driver` (`580.178.04`)** for bare-metal Linux/containers, and **`open-vgpu-tesla-p4` (`535.309.01-vgpu-kvm`)** for 50/50 `GRID P4-4Q` KVM vGPU virtualization |
| Integrated Display Adapter | `00:02.0` Intel `i915` (`card1`, `renderD128`, connectors `DP-1`, `HDMI-A-1`, `VGA-1`) |

## 2. NVIDIA Tesla P4 (`GP104GL`) PCI Topology

Inspected via `sysfs` on `0000:01:00.0`:

| Attribute | Observed Value | Notes |
| :--- | :--- | :--- |
| PCI BDF / Class | `0000:01:00.0` (`0x030200`) | 3D Controller (headless datacenter card, zero display outputs) |
| Vendor / Device ID | `0x10de` : `0x1bb3` | NVIDIA Tesla P4 (`GP104GL`) |
| Subsystem IDs | `0x10de` : `0x11d8` | NVIDIA reference board |
| PCI Revision | `0xa1` | Matches Pascal `GP104` A1 silicon |
| Power State | `D0` | Active PCI power state |
| BAR0 | `0xf6000000` (`16 MiB`) | MMIO control registers (`PMC`, `PFB`, `PMU`, `SEC2`, `FECS`, etc.) |
| BAR1 | `0xe0000000` (`256 MiB`) | VRAM aperture |
| BAR3 | `0xf0000000` (`32 MiB`) | PRAMIN / indirect framebuffer window |
| Current Link Speed | `2.5 GT/s PCIe` (`lnk_speed=1` under `nouveau` / `no_driver`) -> **`8.0 GT/s PCIe` x16 (`lnk_speed=3`) under R580** | Live PMU `5e00` / ACR init required to train PCIe Gen 3 |
| Max Link Speed / Width | `8.0 GT/s PCIe` x `16` | Full PCIe 3.0 x16 capability verified under R580 (`580.178.04`, `32C`, `6W / 75W`, `8192 MiB`) |
| `/sys/bus/pci/devices/0000:01:00.0/rom` | `Exists: False` | Hardware ROM extracted from `nouveau` `vbios.rom` (`239,104`B, `SHA 4e3a141d...`) and direct BAR0 `NV_PROM` (`0x300000`, `262,144`B `prom_raw.bin`) |

## 3. `/lib/firmware/nvidia/gp104/` Gap Analysis

Auditing every `.zst` file under `/lib/firmware/nvidia/gp104/` (with symlinks resolved and decompressed via `zstd -dc`) reveals why open-source `nouveau` leaves the Tesla P4 throttled to PCIe Gen 1 and lacking PMU runtime control (`dmesg` `pmu: firmware unavailable`):

| `/lib/firmware/nvidia/gp104/` Path | Resolved Target | Uncompressed Size | SHA-256 (first 16 hex) |
| :--- | :--- | ---: | :--- |
| `acr/bl.bin.zst` | `gp102/acr/bl.bin.zst` | 1,280 | `01326cc997716beb` |
| `acr/ucode_load.bin.zst` | `gp102/acr/ucode_load.bin.zst` | 17,152 | `5d2ce92a781c6c42` |
| `acr/ucode_unload.bin.zst` | `gp102/acr/ucode_unload.bin.zst` | 3,328 | `1f139e5ada193fc1` |
| `acr/unload_bl.bin.zst` | `gp102/acr/unload_bl.bin.zst` | 1,280 | `dcee20bfa564da6a` |
| `nvdec/scrubber.bin.zst` | `gp102/nvdec/scrubber.bin.zst` | 3,840 | `0f70a43c184d6db4` |
| `sec2/desc.bin.zst` | `gp102/sec2/desc.bin.zst` | 656 | `33116677910e9cc9` |
| `sec2/desc-1.bin.zst` | `gp102/sec2/desc-1.bin.zst` | 656 | `0d0ce7988281fc91` |
| `sec2/image.bin.zst` | `gp102/sec2/image.bin.zst` | 99,072 | `9ca3293e0d11dcf0` |
| `sec2/image-1.bin.zst` | `gp102/sec2/image-1.bin.zst` | 109,568 | `90404a9c365eb9b2` |
| `sec2/sig.bin.zst` | `gp102/sec2/sig.bin.zst` | 192 | `6611a8ff65b34aad` |
| `sec2/sig-1.bin.zst` | `gp102/sec2/sig-1.bin.zst` | 192 | `14c3524427b08a6d` |
| `gr/fecs_bl.bin.zst` | `gm200/gr/fecs_bl.bin.zst` | 576 | `58a14906a9ad69a1` |
| `gr/fecs_data.bin.zst` | `gp104/gr/fecs_data.bin.zst` | 2,576 | `5dc0528718dcff24` |
| `gr/fecs_inst.bin.zst` | `gp104/gr/fecs_inst.bin.zst` | 22,760 | `d4f0706e226fd447` |
| `gr/fecs_sig.bin.zst` | `gp104/gr/fecs_sig.bin.zst` | 192 | `9a47560b4a60c6c2` |
| `gr/gpccs_bl.bin.zst` | `gm200/gr/gpccs_bl.bin.zst` | 576 | `af1d42219e8184c1` |
| `gr/gpccs_data.bin.zst` | `gp104/gr/gpccs_data.bin.zst` | 1,832 | `6db54b7a0b75ac9f` |
| `gr/gpccs_inst.bin.zst` | `gp102/gr/gpccs_inst.bin.zst` | 13,307 | `36ad0ee74de6b79a` |
| `gr/gpccs_sig.bin.zst` | `gp104/gr/gpccs_sig.bin.zst` | 192 | `3468062a8f2e02b4` |
| `gr/sw_*` (`bundle_init`, `ctx`, `method_init`, `nonctx`) | `gp102/gr/sw_*.bin.zst` | 2,496 – 12,288 | (context init netlist) |
| **`pmu/`** | **ABSENT** | **0** | **Not published in `linux-firmware` for `gp104`** |

### Key Observation

Comparing `/lib/firmware/nvidia/gp104/` against our R580.178.04 `.rodata` extraction ([`02-BINARY-AND-FIRMWARE-ANALYSIS.md`](./02-BINARY-AND-FIRMWARE-ANALYSIS.md) and [`06-FALCON-BLOBS-AND-VBIOS-INTERNALS.md`](./06-FALCON-BLOBS-AND-VBIOS-INTERNALS.md)):
1. **`pmu/` firmware is completely absent** in `linux-firmware` for `gp104`, even though R580.178.04 ships a full `300,288`-byte signed `GP104` PMU RTOS image (`0x26108`), `940`-byte descriptor, `768`-byte bootloader (`0x25c80`), and `192`-byte LS signature (`0x2dac0`, `fid = 0`, SHA-256 `5f61eef8f1749300...`) — alongside separate `NVENC` blobs (`0x29ae8`, `0x29b00`, LS sig `0x29f80`, `fid = 4`, SHA-256 `2ebe0dfe3cf9ea29...`) that `nouveau` leaves unimplemented.
2. Even for shared `acr/` (`17,152`-byte `ucode_load.bin`) and `sec2/` (`109,568`-byte `image-1.bin` vs `221,440`-byte `id_0x23.bin` + `65,792`-byte HS blob), R580.178.04 ships significantly newer signed Falcon revisions than the legacy `gp102` symlinks in Ubuntu's `linux-firmware`.
3. Hardware `NV_PROM` (`prom_raw.bin`) also exposes the live card's VBIOS (`G414.0200.00.03`, `3` embedded BIT `'p'` Falcon apps) and trailing `JFFS` InfoROM (`OBD` `Tesla P4`, `ECC`, `PBL`, `BBX`), archived cleanly under [`firmware-blobs/gp104/vbios/`](../firmware-blobs/gp104/vbios/).

