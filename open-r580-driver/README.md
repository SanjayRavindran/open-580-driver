# Open R580 (`580.178.04`) Hybrid Kernel Driver, DKMS Compatibility Layer & Selkies/PixelFlux 10-bit 4:4:4 Runtime

> **Why `open-r580-driver` Exists:**  
> NVIDIA Tesla P4 (`GP104GL`, PCI `10de:1bb3`) enforces fused Production Secure Boot (`NV_FUSE[0x21400] = 0x00000001`). Hardware **requires** NVIDIA's HS/LS signed Falcon blobs to lock `WPR1` (`sctl = 0x7021`), train PCIe Gen 3 (`8.0 GT/s x16`), and unlock PMU/SEC2/NVENC/NVDEC/FECS/GPCCS. Furthermore, CUDA/NVENC/NVDEC/Wayland userspace libraries (`libcuda.so.1`, `libnvidia-encode.so.1`, `libnvcuvid.so.1`, `libEGL_nvidia.so.0`) communicate exclusively with NVIDIA's RM/UVM/DRM ioctl interface.

`open-r580-driver` separates:
1. **Open-Source Linux Kernel Adaptation Layer (`580.178.04/kernel/`)** — MIT / Dual MIT-GPLv2 C wrappers (`nvidia-drm`, `nvidia-uvm`, `os-interface`, `conftest.sh`, DMA-buf, Linux `7.0+` VM/DRM hooks).
2. **Verified Proprietary Core (`nv-kernel.o_binary` & `nv-modeset-kernel.o_binary`)** — verified deterministically by SHA-256 checksum inside [`open_r580_dkms_builder.sh`](./open_r580_dkms_builder.sh), paired with a non-clobbering `--with-userspace` runtime installer that preserves distro GLVND/OpenCL dispatchers, installs Wayland/GBM external platform libraries (`libnvidia-egl-wayland 1.1.20`, `libnvidia-egl-wayland2 1.0.2`, `libnvidia-egl-gbm 1.1.3`), and auto-refreshes NVIDIA Container Toolkit CDI specifications (`/etc/cdi/nvidia.yaml` & `/var/run/cdi/nvidia.yaml`).

## Contents

| File | Purpose |
| :--- | :--- |
| [`open_r580_dkms_builder.sh`](./open_r580_dkms_builder.sh) | SHA-256 verifies R580 `.o_binary` cores, stages `dkms.conf`, Kbuild-tests (`--dry-run`), installs DKMS + non-clobbering userspace + CDI (`--install --with-userspace`), or cleanly removes everything (`--uninstall`). |
| [`dkms.conf.template`](./dkms.conf.template) | Quotes `'make'` cleanly for `dkms(8)` and builds `nvidia`, `nvidia-modeset`, `nvidia-uvm`, `nvidia-drm`. |
| [`modprobe-blacklist-nouveau.conf`](./modprobe-blacklist-nouveau.conf) | Disables `nouveau`, sets `nvidia-drm modeset=1`, and adds `softdep nvidia-drm pre: drm_ttm_helper` required on Linux `7.0+`. |
| [`70-nvidia-p4.rules`](./70-nvidia-p4.rules) | Auto-loads `nvidia-uvm` on `10de:1bb3` bind and configures `0666` `/dev/nvidia*` permissions while preserving integrated GPU (`i915`). |
| [`verify_ioctl_abi.py`](./verify_ioctl_abi.py) | Verifies open kernel header ioctl magic (`'F'`), `NV_ESC_*`, `DRM_IOCTL_NVIDIA_*`, conftest symbol counts, and live `/dev/nvidia*` nodes. |
| [`p4_cuda_nvenc_smoke.c`](./p4_cuda_nvenc_smoke.c) | Strict `-Wall -Wextra -Werror` runtime verifier exercising `libcuda.so.1` VRAM DMA (`0x5a`) + `libnvidia-encode.so.1` (`NVENC 13.0`). |
| [`p4_nvdec_smoke.c`](./p4_nvdec_smoke.c) | Dynamic CUVID (`libnvcuvid.so.1`) verifier checking Pascal GP104 NVDEC hardware decode capabilities (`H.264`, `HEVC` 8/10-bit, `VP9`, `MPEG2`, `JPEG`). |
| [`docker-compose.yml`](./docker-compose.yml) | Reproducible GPU-passthrough stack for LinuxServer Selkies + PixelFlux (`:3000` / `:3001`, configured for **`10-bit` & `4:4:4` `YUV444` NVENC hardware acceleration**) and Jellyfin (`:8096`) via NVIDIA Container Toolkit (`nvidia` runtime). |

## Verified Results on Ubuntu 26.04 (`Linux 7.0.0-43-generic`)

- **DKMS & Userspace Install:** `nvidia-gp104-r580/580.178.04, 7.0.0-43-generic, x86_64: installed`, `nvidia_drm.modeset=Y`, `nvidia-smi` reporting `580.178.04` / `CUDA 13.0`, Intel `i915` (`card1`/`renderD128`) undisturbed alongside NVIDIA (`card0`/`renderD129`).
- **CUDA, NVENC & NVDEC:** `CUDA[0]: Tesla P4 (8109 MiB)`, `VRAM DMA check: 5a 5a 5a 5a`, `NVENC API v13.0: session active, codec GUID count=2` (H.264 & HEVC), `cuvidGetDecoderCaps` hardware decode verified for H.264, HEVC 8/10-bit, VP9, MPEG2, JPEG.
- **NVIDIA Container Toolkit (`1.20.2`) + Selkies (`2.2.1`) + LinuxServer.io PixelFlux (`2.2.3`) with `10-bit` & `4:4:4` Color Depth:**
  - `pixelflux.hardware_formats`: `{'h264': ['420-8', '444-8'], 'h265': ['420-8', '420-10', '444-8', '444-10']}`.
  - With `SELKIES_ENCODER=h265enc,h264enc`, `SELKIES_VIDEO_FULLCOLOR=true`, and `SELKIES_VIDEO_10BIT=true`, PixelFlux performs zero-copy DMA-BUF (`DRI3` -> `CUDA-array` -> `NVENC`) hardware encoding in **`NVENC H265 4:4:4 10-bit`** (`444-10`) and **`NVENC H264 4:4:4 8-bit`** (`444-8`).
