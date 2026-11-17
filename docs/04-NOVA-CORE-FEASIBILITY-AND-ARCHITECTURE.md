# 04 — NVIDIA `nova-core` Feasibility & Pascal (`GP104`) Extension Design

## 1. Short Answer: Can Tesla P4 Work with `nova-core`?

- **Stock upstream `nova-core` (`Linux v7.0`)**: **No.** Upstream `nova-core` supports only **Turing (`TU102`) and newer** microarchitectures (`Architecture` >= Turing), explicitly gating probe on RISC-V **GSP-RM** hardware that Pascal (`GP104`) does not possess. Moreover, stock Ubuntu `26.04.1 LTS` (`7.0.0-43-generic`) currently ships `# CONFIG_NOVA_CORE is not set`.
- **Clean-room Pascal extension (`nova-pascal` pattern)**: **Yes, technically feasible for open Vulkan/OpenGL/video planes — with an important CUDA/NVENC caveat.** Nearly all low-level Rust MMIO/Falcon primitives inside `nova-core` transfer cleanly to `GP104` when `Gsp::boot()` + `FWSEC-FRTS` are swapped for Pascal `DEVINIT` + `WPR1` + Falcon HS `ACR` + isolated signed blobs. **However**, even with signed Falcon blobs loaded inside `nova-pascal`, **closed NVIDIA userspace libraries (`libcuda.so.1`, `libnvidia-encode.so.1`) will NOT work** unless a driver either preserves NVIDIA's proprietary `nvidia.ko` / `nvidia-uvm.ko` ioctl ABI (**Track 1: [`open-r580-driver/`](../open-r580-driver/)**) or implements a complete UMD-compatible RM/UVM ioctl translation layer. Open-source Vulkan (`NVK` / Mesa) + OpenGL (`Zink`) + open video paths can target clean-room DRM ioctls cleanly.

## 2. Exact Hardware & Firmware Deltas: Pascal (`GP104`) vs. Turing+ (`nova-core`)

| Subsystem | Upstream `nova-core` (`TU102+`) | Tesla P4 (`GP104`, `0x134`) | Portability / Work Required |
| :--- | :--- | :--- | :--- |
| **Chip ID (`NV_PMC_BOOT_42` `0xa00`)** | `arch >= 0x16` (`Turing`+) | `arch = 0x13`, `impl = 0x4` (`GP104`) | Direct: add `Architecture::Pascal` + `Chipset::GP104` to `gpu.rs` |
| **VRAM & Sysmem Flush (`0x100ce0` / `0x100c10`)** | `FbHal` + `SysmemFlush` | Same `NV_PFB_PRI_MMU_LOCAL_MEMORY_RANGE` (`0x100ce0`) & NISO flush (`0x100c10`) | Direct reuse (`FbLayout`, `SysmemFlush`) |
| **VBIOS (`BIT` + Devinit)** | `PCIR` + `BIT` + `FWSEC-FRTS` (`0xe0`) | Legacy `PCIR` (`0x00`) + `BIT` tokens (`0x49`, `0x70`); **no `FWSEC-FRTS`**; `/sys/.../rom` absent (`Exists: False`), read via `PROM`/`PRAMIN` | Adapt VBIOS loader to read `PRAMIN`/`PROM` when sysfs ROM absent; skip `FWSEC` |
| **Falcon Base Map** | `SEC2` at `0x0084_0000`, `GSP` Falcon | `SEC2` at **`0x0008_7000`**, `PMU` at `0x0010_a000`, `NVDEC` at `0x0008_4000`, **`NVENC0` at `0x001c_8000`**, `FECS` at `0x0040_9000`, `GPCCS` at `0x0041_a000`; **`FALCON_SCTL` (`0x240`)** (`0x3000 -> 0x7021`) | Parameterized in [`falcon.rs`](file:///c:/Users/Sanjay/Documents/AI%20Projects/p4/nova-pascal/src/falcon.rs) |
| **Security & Firmware Boot** | `Booter` -> RISC-V `GSP-RM` handles all engines & reclocking | Host carves `WPR1` (`0x100e00`), builds `LsfWprHeader`/`LsfLsbHeader`, patches HS `PROD`/`DBG` sig (`0x3a20`), boots HS `ACR_LOAD` on `PMU`/`SEC2` -> unlocks `PMU_RTOS` (`300,288B`) + `SEC2` + `FECS`/`GPCCS` | Implemented clean-room in [`nova-pascal/src/{falcon,acr,boot}.rs`](file:///c:/Users/Sanjay/Documents/AI%20Projects/p4/nova-pascal/src/boot.rs) |
| **Post-Boot Control Plane (Host RM vs. GSP)** | Thin RPC client forwarding ioctls/MMU/GR/FIFO to `GSP-RM` | **Must implement host-side GPU control**: Bar1/VRAM buddy allocator, 2-level Pascal GMMU page tables, FIFO/Host1x channel ring (`GPFIFO`), `GR`/`CE` context init, `PMU` RPCs (`0xa00`) for P-states/reclocking, `NVDEC`/`NVENC` engine channels | **Major engineering delta**: ~60–75% of a full DRM/compute driver (already solved natively inside `nouveau` except missing GP104 PMU/NVENC support) |

## 3. Clean-Room `nova-pascal` Prototype Architecture

The clean-room Rust crate [`nova-pascal`](file:///c:/Users/Sanjay/Documents/AI%20Projects/p4/nova-pascal/Cargo.toml) (`#![cfg_attr(not(test), no_std)]`, zero external crate dependencies) mirrors `nova-core`'s layered trait design so it can drop into Linux kernel Rust or run userspace/MMIO simulations identically:

- [`regs.rs`](file:///c:/Users/Sanjay/Documents/AI%20Projects/p4/nova-pascal/src/regs.rs): `Bar0Io` MMIO trait + verified Pascal BAR0 constants (`NV_PMC_BOOT_0`, `NV_PMC_BOOT_42`, `NV_FUSE_PRIV_SEC_STATUS = 0x21400`, `NV_PROM_DATA_BASE = 0x300000`, `NV_PFB_PRI_MMU_WPR_PRIV_BASE`, `is_pri_error(0xbadf5040)`).
- [`gpu.rs`](file:///c:/Users/Sanjay/Documents/AI%20Projects/p4/nova-pascal/src/gpu.rs): Uniform table mapping `(arch, impl)` -> `Spec` (`Chipset::GP104 = 0x134`, `rm_tag = Some(0x1f)`, `BootPath::PascalAcrHostRm`).
- [`fb.rs`](file:///c:/Users/Sanjay/Documents/AI%20Projects/p4/nova-pascal/src/fb.rs): `FbLayout::vidmem_size_from_reg()`, `128 KiB`-aligned `WPR1` placement (`plan_wpr1`), and `SysmemFlush::flush()`.
- [`vbios.rs`](file:///c:/Users/Sanjay/Documents/AI%20Projects/p4/nova-pascal/src/vbios.rs): Zero-allocation `PCIR` chain walker, EFI GOP (`+0x10a00`) `BIT 0x70` Falcon Ucode Table offset translator (`parse_vbios_falcon_apps`), and `NV_PROM` `JFFS` InfoROM directory scanner (`parse_prom_inforom`).
- [`firmware.rs`](file:///c:/Users/Sanjay/Documents/AI%20Projects/p4/nova-pascal/src/firmware.rs): Clean-room binary header parsers for `24B` `FlcnBlDesc`, `36B` `AcrHsDesc`, `v2/v4` `RtosDesc`, `192B` `LsfSigInfo` XOR footer (`0xe931c2b6`), and packed `GR` Netlist TOC (`parse_gr_netlist_toc`).
- [`falcon.rs`](file:///c:/Users/Sanjay/Documents/AI%20Projects/p4/nova-pascal/src/falcon.rs): `FalconEngine::bar0_base(arch)` (including `Sec2 = 0x087000` and `Nvenc0 = 0x1c8000`), `FALCON_SCTL` (`0x240`) `is_ls_mode()` (`0x7021`), `wait_scrub()`, PIO IMEM/DMEM loaders, `start()`, and `wait_halt()` (`MAILBOX0 = 0x4f`).
- [`acr.rs`](file:///c:/Users/Sanjay/Documents/AI%20Projects/p4/nova-pascal/src/acr.rs): `FuseStatus::probe()` (`NV_FUSE[0x21400]`), `FuseMode` (`Production` vs `Debug`) pre-signed HS patcher, and `LsFalconId` (`Pmu=0, Fecs=2, Gpccs=3, Nvenc=4, Nvdec=5, Sec2=7`).
- [`boot.rs`](file:///c:/Users/Sanjay/Documents/AI%20Projects/p4/nova-pascal/src/boot.rs): End-to-end `run_pascal_bootstrap()` state machine replacing `Gsp::boot()` for `BootPath::PascalAcrHostRm`.

## 4. Pragmatic Engineering Verdict on `nova-core`

Writing an out-of-tree Pascal `nova-core` boot crate (Stage 1: Falcon + `WPR1` + `PMU`/`SEC2` init) is compact and completely achievable (`nova-pascal` demonstrates the core bootstrap in ~500 lines of `no_std` Rust). However, completing **all** post-boot host-RM planes (`GMMU`, `FIFO`, `GR`, `CE`, `PMU` DVFS, `NVENC`, Vulkan/CUDA UMD ABI) inside `nova` duplicates nearly everything **`nouveau` already maintains upstream in C**—except for the specific missing links documented in [`05-KERNEL-LONGEVITY-AND-REMOTE-DESKTOP.md`](file:///c:/Users/Sanjay/Documents/AI%20Projects/p4/docs/05-KERNEL-LONGEVITY-AND-REMOTE-DESKTOP.md).

