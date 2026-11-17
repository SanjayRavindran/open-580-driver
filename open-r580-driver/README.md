# Track 1: Open R580 (`580.178.04`) Hybrid Kernel Driver & DKMS Compatibility Layer

> **Why Track 1 Exists:**  
> NVIDIA Tesla P4 (`GP104GL`, PCI `10de:1bb3`) enforces fused Production Secure Boot (`NV_FUSE[0x21400] = 0x00000001`). Hardware **requires** NVIDIA's HS/LS signed Falcon blobs to lock `WPR1` (`sctl = 0x7021`), train PCIe Gen 3 (`8.0 GT/s x16`), and unlock PMU/SEC2/NVENC/FECS/GPCCS. Furthermore, closed proprietary userspace libraries (`libcuda.so.1`, `libnvidia-encode.so.1`) communicate exclusively with NVIDIA's RM/UVM/DRM ioctl interface.

Track 1 separates:
1. **Open-Source Linux Kernel Adaptation Layer (`580.178.04/kernel/`)** — MIT / Dual MIT-GPLv2 C wrappers (`nvidia-drm`, `nvidia-uvm`, `os-interface`, `conftest.sh`, DMA-buf, Linux `7.0+` VM/DRM hooks). Note: NVIDIA's `580.178.04/kernel-open/` flavor omits all pre-Turing HALs (`17 MiB` vs `108 MiB` `nv-kernel.o_binary`) and explicitly rejects pre-GSP architectures (`html/kernel_open.html`), so Pascal/GP104 requires the open-shim `kernel/` adaptation layer + verified core blobs.
2. **Verified Proprietary Core (`nv-kernel.o_binary` & `nv-modeset-kernel.o_binary`)** — verified deterministically by SHA-256 checksum inside [`open_r580_dkms_builder.sh`](./open_r580_dkms_builder.sh), paired with a non-clobbering `--with-userspace` runtime installer (tracks installed vendor files in `/var/lib/nvidia-gp104-r580/installed.manifest` while preserving Ubuntu GLVND/OpenCL dispatchers and Intel `i915` rendering).

## Contents

| File | Purpose |
| :--- | :--- |
| [`open_r580_dkms_builder.sh`](./open_r580_dkms_builder.sh) | SHA-256 verifies R580 `.o_binary` cores, stages `dkms.conf`, Kbuild-tests (`--dry-run`), installs DKMS + non-clobbering userspace (`--install --with-userspace`), or cleanly removes everything (`--uninstall`). |
| [`dkms.conf.template`](./dkms.conf.template) | Quotes `'make'` cleanly for `dkms(8)` and builds `nvidia`, `nvidia-modeset`, `nvidia-uvm`, `nvidia-drm` (`nvidia-peermem` excluded cleanly on standard GP104 hosts). |
| [`modprobe-blacklist-nouveau.conf`](./modprobe-blacklist-nouveau.conf) | Disables `nouveau`, sets `nvidia-drm modeset=1`, and adds `softdep nvidia-drm pre: drm_ttm_helper` observed as required on Linux `7.0.0-43-generic`. |
| [`70-nvidia-p4.rules`](./70-nvidia-p4.rules) | Auto-loads `nvidia-uvm` on `10de:1bb3` bind and configures `0666` `/dev/nvidia*` permissions (for rootless/container workloads) while preserving integrated GPU (`i915`). |
| [`verify_ioctl_abi.py`](./verify_ioctl_abi.py) | Verifies open kernel header ioctl magic (`'F'`), `NV_ESC_*`, `DRM_IOCTL_NVIDIA_*`, conftest symbol counts, and live `/dev/nvidia*` nodes. |
| [`p4_cuda_nvenc_smoke.c`](./p4_cuda_nvenc_smoke.c) | Strict `-Wall -Wextra -Werror` runtime verifier exercising `libcuda.so.1` VRAM DMA (`0x5a`) + `libnvidia-encode.so.1` (`NVENC 13.0`). |
| [`p4_nvdec_smoke.c`](./p4_nvdec_smoke.c) | Dynamic CUVID (`libnvcuvid.so.1`) verifier checking Pascal GP104 NVDEC hardware decode capabilities (`H.264`, `HEVC` 8/10-bit, `VP9`, `MPEG2`, `JPEG`, and expected `AV1=0`). |
| [`docker-compose.yml`](./docker-compose.yml) | Reproducible GPU-passthrough stack for LinuxServer Selkies + PixelFlux (`:3000` / `:3001`) and Jellyfin (`:8096`) via NVIDIA Container Toolkit (`nvidia` runtime). |

## Verified Results on Ubuntu 26.04 (`Linux 7.0.0-43-generic`)

- **DKMS & Userspace Install:** `nvidia-gp104-r580/580.178.04, 7.0.0-43-generic, x86_64: installed` (auto-signed with host MOK), `nvidia_drm.modeset=Y`, `103` non-clobbering userspace paths tracked, `nvidia-smi` reporting `580.178.04` / `CUDA 13.0`, Intel `i915` (`card1`/`renderD128`) undisturbed alongside NVIDIA (`card0`/`renderD129`).
- **CUDA & NVENC (`p4_cuda_nvenc_smoke`):** `CUDA[0]: Tesla P4 (8109 MiB)`, `VRAM DMA check: 5a 5a 5a 5a`, `NVENC API v13.0: session active, codec GUID count=2`.
- **NVDEC (`p4_nvdec_smoke` & Host FFmpeg):** `cuvidGetDecoderCaps` confirmed hardware decode for H.264 (`max=4096x4096`), HEVC 8-bit & 10-bit (`max=8192x8192`), VP9 (`max=8192x8192`), MPEG2, JPEG (`AV1=0`); host FFmpeg ran `h264_nvenc`, `hevc_nvenc`, `h264_cuvid`, and GPU-resident `hevc_cuvid -> h264_nvenc` (`60` frames, `731K`).
- **Docker `29.1.3` + NVIDIA Container Toolkit `1.20.2`:**
  - `selkies-webtop` (`lscr.io/linuxserver/webtop:ubuntu-xfce`): `Selkies 2.2.1` + `pixelflux 2.2.3` initialized zero-copy DRI3 capture on `/dev/dri/renderD129` -> NVENC H.264 (`2` engines), `SELKIES_GPU_PRESENT=true`, `SELKIES_GPU_MESA_DRIVER=zink` (`Vulkan 1.4` Tesla P4), HTTP/HTTPS `200`, screenshot `1024x768` PNG, live WebSocket binary H.264 chunks streamed.
  - `jellyfin` (`lscr.io/linuxserver/jellyfin:latest` `12.4.0`): `ffmpeg 8.1.3-Jellyfin` ran 1080p NVENC (`h264_nvenc` `4.69x`, `hevc_nvenc` `6.68x`) and GPU-resident NVDEC->NVENC transcodes (`hevc_cuvid -> scale_cuda=1280:720 -> h264_nvenc` and `h264_cuvid -> hevc_nvenc` `6.92x`).

