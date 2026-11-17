# 07 — Track 1 Architecture: Open R580 Hybrid Driver, DKMS & Live CUDA/NVENC

## 1. Why Both Track 1 (`open-r580-driver`) and Track 2 (`nova-pascal`) Exist

Because fused Tesla P4 silicon enforces Production Secure Boot (`NV_FUSE[0x21400] = 1`) and `PRIV_LEVEL_MASK` (`0xbadf5040`) across all Falcon/WPR domains, and NVIDIA's proprietary userspace libraries (`libcuda.so.1`, `libnvidia-encode.so.1`) speak exclusively to the RM/UVM `'F'` ioctl ABI:

| Goal / Workload | Track 1: [`open-r580-driver/`](../open-r580-driver/) | Track 2: [`nova-pascal/`](../nova-pascal/) (`+ nouveau/NVK`) |
| :--- | :--- | :--- |
| **Bypass Falcon in Software** | **Impossible on fused retail silicon** (hardware BROM/WPR blocks) | **Impossible on fused retail silicon** (uses isolated signed blobs cleanly) |
| **CUDA Compute & VRAM DMA (`libcuda.so.1`)** | **Yes — Live-verified CUDA `13.0` (`8,109 MiB` usable)** | Not natively (would require re-implementing RM/UVM ioctl ABI + compiler stack) |
| **NVENC Hardware Encode (`libnvidia-encode.so.1`)** | **Yes — Live-verified `NVENC 13.0` (`2` hardware codec GUIDs)** | Not drop-in proprietary NVENC; open `nouveau`/VA-API path must drive Falcon `NVENC0` (`0x1c8000`) |
| **PCIe Gen 3 (`8.0 GT/s x16`) & Full Power/Clocks** | **Yes — Live-verified `lnk_speed=3` (`8.0 GT/s`)** | Requires PMU RTOS (`0x2dac0` + `300,288`B) bootstrap |
| **Open-Source Kernel Longevity (`Linux 7.x+`)** | MIT/GPLv2 kernel C shim (`kernel/`) handles Linux MM/DRM churn cleanly | Pure open Rust (`no_std`, zero proprietary kernel linkage) |

## 2. Clean License & ABI Split Inside `580.178.04/kernel/`

Building `580.178.04/kernel/` against **Ubuntu `26.04.1 LTS` (`Linux 7.0.0-43-generic`)** produced all kernel modules cleanly (`exit 0`) via Kbuild with zero source patches:

- `nvidia.ko` (`license=NVIDIA`, links `nv-kernel.o_binary` `SHA-256 5269a23f...` to the open C shim `nv-interface.o`)
- `nvidia-modeset.ko` (`license=NVIDIA`, `depends=video,nvidia`, links `nv-modeset-kernel.o_binary` `SHA-256 9a8640a8...`)
- `nvidia-drm.ko` (`license=MIT`, `depends=nvidia-modeset,drm_ttm_helper,nvidia`)
- `nvidia-uvm.ko` (`license=Dual MIT/GPL`, `depends=nvidia`)

### Linux `7.0.0-43-generic` Integration Fix Observed Live
When loading modules manually on Linux `7.0.0-43-generic`, `nvidia-drm.ko` failed with `-ENOENT` (`Unknown symbol`) until `drm_ttm_helper` was pre-loaded (`modprobe drm_ttm_helper`). Track 1's [`modprobe-blacklist-nouveau.conf`](../open-r580-driver/modprobe-blacklist-nouveau.conf) embeds explicit `softdep nvidia-drm pre: drm_ttm_helper` alongside blacklisting `nouveau`, and keeps Intel integrated graphics (`i915`, `card1`/`renderD128`) untouched for console display.

## 3. Track 1 Toolkit Workflow ([`open-r580-driver/`](../open-r580-driver/))

1. **[`open_r580_dkms_builder.sh`](../open-r580-driver/open_r580_dkms_builder.sh)**:
   - Verifies exact SHA-256 digests of `nv-kernel.o_binary` and `nv-modeset-kernel.o_binary`.
   - Stages [`dkms.conf.template`](../open-r580-driver/dkms.conf.template) (using quoted `'make'` so `dkms(8)` does not inject confusing `KERNELRELEASE` overrides, and excluding `nvidia-peermem` by default).
   - Supports non-destructive `--dry-run` compilation verification or full `--install` setup (DKMS registration + `modprobe.d` + [`70-nvidia-p4.rules`](../open-r580-driver/70-nvidia-p4.rules)).
2. **[`verify_ioctl_abi.py`](../open-r580-driver/verify_ioctl_abi.py)**:
   - Parses `common/inc/nv-ioctl-numbers.h` (`NV_IOCTL_MAGIC = 'F'`, all `NV_ESC_*` escapes), `nv-ioctl-numa.h`, `nvidia-drm/nvidia-drm-ioctl.h`, and `conftest/*.h` define/undef counts.
   - Supports `--live` inspection of `/dev/nvidiactl`, `/dev/nvidia0`, `/dev/nvidia-uvm`, `/dev/nvidia-modeset`, and sysfs PCI driver binding.
3. **[`p4_cuda_nvenc_smoke.c`](../open-r580-driver/p4_cuda_nvenc_smoke.c)**:
   - Compiles cleanly under `gcc -Wall -Wextra -Werror -O2 -ldl` against `libcuda.so.1` and `libnvidia-encode.so.1`.
   - Note from disassembly of `libnvidia-encode.so.580.178.04` (`NvEncodeAPICreateInstance` `0x23a20`): modern session creation invokes `ptrs[30]` (`+0xf0`, `nvEncOpenEncodeSessionEx`) with a `CUDA` context (`deviceType = 1`) and queries `ptrs[2]` (`+0x10`, `nvEncGetEncodeGUIDCount`), whereas legacy `ptrs[1]` (`+0x08`) rejects headless `CUDA` contexts.
   - Observed live on Tesla P4 (`Linux 7.0.0-43-generic`):
     - `CUDA[0]: Tesla P4 (8109 MiB)`
     - `VRAM DMA check: 5a 5a 5a 5a`
     - `NVENC API v13.0: session active, codec GUID count=2` (H.264 & HEVC)

4. **[`p4_nvdec_smoke.c`](../open-r580-driver/p4_nvdec_smoke.c) & `ffmpeg` Encode/Decode Verification**:
   - Dynamic CUVID (`libnvcuvid.so.1`) capability probe confirmed GP104 hardware decode support for `H.264 8-bit` (`4096x4096`), `HEVC 8-bit` & `10-bit` (`8192x8192`), `VP9 8-bit` (`8192x8192`), `MPEG2` (`4080x4080`), and `JPEG` (`AV1=0`, as hardware-expected on Pascal).
   - Host `ffmpeg` verified standalone `h264_nvenc`, `hevc_nvenc`, `h264_cuvid`, and GPU-resident `hevc_cuvid -> h264_nvenc` (`60` frames, `731 KiB`).

## 4. Non-Support of `kernel-open` vs Clean DKMS & Userspace Install

- **Why NVIDIA `kernel-open` (`580.178.04/kernel-open/`) cannot drive Tesla P4:** NVIDIA's extracted documentation (`html/kernel_open.html`) explicitly states that open kernel modules require a GPU System Processor (GSP) introduced in Turing (`nv-kernel.o_binary` in `kernel-open/` is `17 MiB` vs `108 MiB` in `kernel/` and omits pre-Turing RM/HAL support).
- **Clean Track 1 Host Deployment (`--install --with-userspace`):**
  - Unloads any temporary/ad-hoc `.ko` builds, installs `nvidia-gp104-r580/580.178.04` via DKMS (auto-signed for `7.0.0-43-generic`), writes persistent `/etc/modprobe.d/blacklist-nouveau.conf`, `/etc/modules-load.d/nvidia-p4.conf`, `/etc/udev/rules.d/70-nvidia-p4.rules` (`0666` `/dev/nvidia*`), updates `initramfs`, and installs `103` vendor paths (`libcuda`, `libnvidia-encode`, `libnvcuvid`, `libnvidia-ml`, `libnvidia-fbc`, `libGLX_nvidia`, `libEGL_nvidia`, Vulkan/EGL/GBM/OpenCL ICDs, `nvidia-smi`, `04755` `nvidia-modprobe`) tracked in `/var/lib/nvidia-gp104-r580/installed.manifest` **without overwriting** system `libGL.so.1`/`libEGL.so.1`/`libOpenCL.so.1` or touching Intel `i915` (`card1`/`renderD128`).

## 5. Docker, NVIDIA Container Toolkit, Selkies + PixelFlux & Jellyfin

- **Docker `29.1.3` + `nvidia-container-toolkit` `1.20.2`:** Registered `nvidia-container-runtime` via `nvidia-ctk runtime configure --runtime=docker`, cleanly exposing `/dev/nvidia*`, NVIDIA DRM `card0`/`renderD129` (omitting host Intel `renderD128`), and driver libraries into GPU containers.
- **Selkies `2.2.1` + PixelFlux `2.2.3` (`lscr.io/linuxserver/webtop:ubuntu-xfce`):** Launched on ports `3000`/`3001` with `--runtime=nvidia --gpus all --device /dev/nvidia-modeset -e DRINODE=/dev/dri/renderD129`. Live verification confirmed:
  - `selkies.gpu_probe`: `SELKIES_GPU_DRIVER=nvidia`, `SELKIES_GPU_PRESENT=true`, `SELKIES_GPU_ACCELERATED=true`, `SELKIES_GPU_MESA_DRIVER=zink`, `SELKIES_GPU_VULKAN_PRESENTS=true`, `GL_RENDERER: zink Vulkan 1.4(Tesla P4 (NVIDIA_PROPRIETARY))`.
  - PixelFlux: `[X11] Encoder: NVENC H264 4:2:0 8-bit on Tesla P4 (render node 1, nvidia driver), 2 engines.` and zero-copy DRI3 capture on `/dev/dri/renderD129`, streaming live H.264 frames over `wss://.../api/websockets` plus HTTP/HTTPS `200` and `/api/screenshot` (`1024x768` PNG).
- **Jellyfin `12.4.0` (`lscr.io/linuxserver/jellyfin:latest`):** Launched on port `8096`; `/System/Info/Public` responded `200 OK` and `ffmpeg version 8.1.3-Jellyfin` completed 1080p NVENC (`h264_nvenc` at `4.69x`, `hevc_nvenc` at `6.68x`) plus end-to-end GPU-resident transcodes (`hevc_cuvid -> scale_cuda=1280:720 -> h264_nvenc` and `h264_cuvid -> hevc_nvenc` at `6.92x`, verified via `ffprobe`). See reproducible [`docker-compose.yml`](../open-r580-driver/docker-compose.yml).

