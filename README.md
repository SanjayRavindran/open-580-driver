# `open-580-driver` — Open-Shim R580 (`580.178.04`) Driver Stack, Falcon Analysis & Selkies/PixelFlux 10-bit 4:4:4 Toolkit for NVIDIA Tesla P4 (`GP104GL`)

This repository contains the **Open-Shim R580 (`580.178.04`) DKMS / CUDA / NVENC / NVDEC / Wayland driver stack**, **silicon Falcon bypass disproof**, **isolated GP104 firmware extraction pipeline**, and **NVIDIA Container Toolkit (`nvidia-ctk`) + Selkies / LinuxServer.io PixelFlux (`10-bit` & `4:4:4` `YUV444`) hardware acceleration integration** for the **NVIDIA Tesla P4 (`GP104GL`, PCI `10de:1bb3`)** on **Ubuntu 26.04.1 LTS (`Linux 7.0.0-43-generic`, UEFI Coreboot host `192.168.1.24`)**.

> [!NOTE]
> For KVM vGPU virtualization (Pure Wayland Linux desktop + Windows 11 WDDM `539.72` with a 50/50 `GRID P4-4Q` split), see the companion [**`open-vgpu-tesla-p4`**](https://github.com/SanjayRavindran/open-vgpu-tesla-p4) repository. The experimental pure `nova-core` / Mesa / Vulkan track has been retired and removed to focus strictly on `open-580-driver` and `open-vgpu-tesla-p4`.

---

## Executive Summary

1. **Why Falcon Signature Verification Cannot Be Bypassed in Software:**
   - Live BAR0 MMIO reads via [`p4_cleanroom_diag.c`](./kmods/p4_cleanroom_diag/p4_cleanroom_diag.c) confirm `NV_FUSE[0x21400] = 0x00000001` (`PrivSec` fused ON), `NV_PFB_WPR_INFO` (`0x100e40..50`) returning **`0xbadf5040`** (`PRIV_LEVEL_MASK` Level-0 read block), and every engine pinned in `sctl = 0x3000` until NVIDIA's HS/LS signed blobs authenticate into Light-Secure (`LS`) mode (`sctl = 0x7021`). All required signed GP104 Falcon blobs are isolated via [`tools/nv_bindata_analyzer.py`](./tools/nv_bindata_analyzer.py) into [`firmware-blobs/`](./firmware-blobs/README.md).

2. **`open-r580-driver/` (`580.178.04` Open Kernel Shims + Verified Blobs + Wayland & Container Runtime):**
   - Separates MIT / Dual MIT-GPL kernel adaptation C shims (`kernel/`) from SHA-256 verified binary cores (`nv-kernel.o_binary`, `nv-modeset-kernel.o_binary`) with Linux `7.0+` `drm_ttm_helper` `softdep`, DKMS config, udev rules, Wayland EGL (`libnvidia-egl-wayland 1.1.20`, `libnvidia-egl-wayland2 1.0.2`, `libnvidia-egl-gbm 1.1.3`), and automatic NVIDIA Container Toolkit CDI generation (`/etc/cdi/nvidia.yaml`).
   - **Live-verified on `Linux 7.0.0-43-generic`**: Falcon `PMU`/`SEC2`/`FECS`/`GPCCS` active at `sctl = 0x7021`, PCIe Gen 3 **`8.0 GT/s` (`lnk_speed=3` x16)**, **`CUDA 13.0` (`8,109 MiB` VRAM DMA pass)**, **`NVENC 13.0` (`H.264` & `HEVC`)**, and **`NVDEC` (`H.264`, `HEVC` 8/10-bit, `VP8`, `VP9`, `MPEG2`, `VC1`, `JPEG`)**.

3. **NVIDIA Container Toolkit + Selkies & LinuxServer.io PixelFlux (`10-bit` & `4:4:4` Color Depth):**
   - Configured in [`docker-compose.yml`](./docker-compose.yml) and [`open-r580-driver/docker-compose.yml`](./open-r580-driver/docker-compose.yml) with `SELKIES_ENCODER=h265enc,h264enc`, `SELKIES_VIDEO_FULLCOLOR=true` (`4:4:4` chroma subsampling), `SELKIES_VIDEO_10BIT=true` (`10-bit` color depth), and `GBM_BACKENDS_PATH=/usr/lib64/gbm:/usr/lib/x86_64-linux-gnu/gbm`.
   - Zero-copy DMA-BUF (`DRI3` -> `CUDA-array` -> `NVENC`) encodes **`NVENC H265 4:4:4 10-bit` (`444-10`)** and **`NVENC H264 4:4:4 8-bit` (`444-8`)** directly in hardware on the Tesla P4.

---

## Repository Layout

| Deliverable | Purpose |
| :--- | :--- |
| [`docs/00-MASTER-PROJECT-COMPREHENSIVE-ARCHIVE.md`](./docs/00-MASTER-PROJECT-COMPREHENSIVE-ARCHIVE.md) | **Master Knowledge Base**: hardware proofs, Falcon security internals, DKMS hybrid driver packaging, and container benchmarks |
| [`docs/10-INSTALLATION-GUIDE-HOST-AND-GUEST.md`](./docs/10-INSTALLATION-GUIDE-HOST-AND-GUEST.md) | **Unified Host & Guest Installation Guide**: step-by-step installation for both `open-580-driver` (`scripts/install_open_580.sh`) and `open-vgpu-tesla-p4` (`scripts/install_open_vgpu.sh`) across Host, Linux Guest, and Windows 11 Guest |
| [`scripts/`](./scripts/install_open_580.sh) | Unified host & guest installers [`install_open_580.sh`](./scripts/install_open_580.sh) (`--role host\|guest-linux`) and [`install_open_vgpu.sh`](./scripts/install_open_vgpu.sh) (`--role host\|guest-linux\|guest-windows`) |
| [`open-r580-driver/`](./open-r580-driver/README.md) | DKMS builder (`--dry-run` / `--install --with-userspace`), `nouveau` blacklist + `drm_ttm_helper` `softdep`, udev rule, `verify_ioctl_abi.py`, `p4_cuda_nvenc_smoke.c`, `p4_nvdec_smoke.c`, & `docker-compose.yml` |
| [`firmware-blobs/`](./firmware-blobs/README.md) | Isolated GP104 Falcon binaries (`acr/`, `pmu/`, `sec2/`, `nvenc/`, `nvdec/`, `gr/`), `gp104_vbios.rom`, `prom_raw.bin`, & [`BLOB_MANIFEST.json`](./firmware-blobs/gp104/BLOB_MANIFEST.json) |
| [`tools/`](./tools/nv_bindata_analyzer.py) | Clean-room ELF64 `.rodata` bindata extractor (`nv_bindata_analyzer.py`) + Falcon/VBIOS/PROM descriptor decoder (`nv_fw_descriptor_parser.py`) |
| [`kmods/p4_cleanroom_diag/`](./kmods/p4_cleanroom_diag/p4_cleanroom_diag.c) | Standalone GPLv2 Linux `7.0` read-only PCI/BAR0/Falcon/VBIOS/PROM diagnostic kernel module |
| [`dkms-compat/`](./dkms-compat/r580-linux-7x-compat-matrix.md) | R580 Linux `7.x` Kbuild verification helper (`verify_r580_kbuild.sh`) and `conftest` / ABI compatibility matrix |
| [`docs/01 – 07`](./docs/07-OPEN-R580-HYBRID-AND-CUDA-NVENC.md) | Technical documentation suite (`01` hardware/UEFI Coreboot inventory, `02` binary analysis, `03` silicon boundary, `05` kernel longevity & remote desktop, `06` Falcon/VBIOS internals, `07` R580 hybrid + Selkies/PixelFlux 10-bit 4:4:4) |
| [`evidence/`](./evidence/rea_evidence_summary.json) | Machine-readable REA/Ghidra digests, hardware telemetry captures, PROM InfoROM metadata, and decompiled GP104 HAL matrix |
| [`LICENSE`](./LICENSE) & [`DISCLAIMER.md`](./DISCLAIMER.md) | GNU GPL v2.0 license, risk notice, and AI/LLM provenance disclosure |
