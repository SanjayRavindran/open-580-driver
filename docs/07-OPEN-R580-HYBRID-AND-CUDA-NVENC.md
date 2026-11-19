# 07 — Open R580 (`580.178.04`) Hybrid Driver, DKMS, CUDA/NVENC & Selkies/PixelFlux 10-bit 4:4:4 Integration

## 1. Architecture Overview

Because fused Tesla P4 silicon enforces Production Secure Boot (`NV_FUSE[0x21400] = 1`) and `PRIV_LEVEL_MASK` (`0xbadf5040`) across all Falcon/WPR domains, and NVIDIA's CUDA/NVENC/NVDEC/Wayland userspace libraries (`libcuda.so.1`, `libnvidia-encode.so.1`, `libnvcuvid.so.1`, `libEGL_nvidia.so.0`) speak exclusively to the RM/UVM `'F'` ioctl ABI, [`open-r580-driver/`](../open-r580-driver/) combines:

- **Open-Source Linux Kernel Shims (`580.178.04/kernel/`)**: MIT / Dual MIT-GPL C adaptation layer (`nv-interface.o`, `nvidia-drm.ko`, `nvidia-uvm.ko`) that adapts to Linux `7.0+` MM/DRM kernel changes cleanly.
- **Verified Binary Cores & Isolated GP104 Falcon Blobs**: SHA-256 verified `nv-kernel.o_binary` and `nv-modeset-kernel.o_binary`, plus isolated signed Falcon microcode (`firmware-blobs/gp104/`).
- **Non-Clobbering Userspace & Automatic NVIDIA Container Toolkit CDI Setup**: Tracks all installed libraries in `/var/lib/nvidia-gp104-r580/installed.manifest`, installs Wayland/GBM external platform libraries (`libnvidia-egl-wayland 1.1.20`, `libnvidia-egl-wayland2 1.0.2`, `libnvidia-egl-gbm 1.1.3`), and auto-generates `/etc/cdi/nvidia.yaml` & `/var/run/cdi/nvidia.yaml` for container workloads.

> [!NOTE]
> The experimental pure `nova-core` / Mesa / Vulkan track has been retired and removed so development focuses strictly on **`open-580-driver`** (bare-metal Linux + container stack) and [**`open-vgpu-tesla-p4`**](https://github.com/SanjayRavindran/open-vgpu-tesla-p4) (KVM vGPU stack for Pure Wayland Linux & Windows 11 WDDM).

---

## 2. Clean License & ABI Split Inside `580.178.04/kernel/`

Building `580.178.04/kernel/` against **Ubuntu `26.04.1 LTS` (`Linux 7.0.0-43-generic`, UEFI Coreboot host `192.168.1.24`)** produces all kernel modules cleanly (`exit 0`) via Kbuild:

- `nvidia.ko` (`license=NVIDIA`, links `nv-kernel.o_binary` `SHA-256 5269a23f...` to the open C shim `nv-interface.o`)
- `nvidia-modeset.ko` (`license=NVIDIA`, `depends=video,nvidia`, links `nv-modeset-kernel.o_binary` `SHA-256 9a8640a8...`)
- `nvidia-drm.ko` (`license=MIT`, `depends=nvidia-modeset,drm_ttm_helper,nvidia`)
- `nvidia-uvm.ko` (`license=Dual MIT/GPL`, `depends=nvidia`)

### Linux `7.0.0-43-generic` Integration Fix
On Linux `7.0.0-43-generic`, `nvidia-drm.ko` requires `drm_ttm_helper` pre-loaded (`softdep nvidia-drm pre: drm_ttm_helper` in [`modprobe-blacklist-nouveau.conf`](../open-r580-driver/modprobe-blacklist-nouveau.conf)).

---

## 3. Verification Suite ([`open-r580-driver/`](../open-r580-driver/))

1. **[`open_r580_dkms_builder.sh`](../open-r580-driver/open_r580_dkms_builder.sh)**:
   - Verifies SHA-256 digests of `nv-kernel.o_binary` and `nv-modeset-kernel.o_binary`.
   - Supports `--dry-run`, `--install --with-userspace` (DKMS + `modprobe.d` + udev rule + non-clobbering userspace + `nvidia-ctk cdi generate`), and `--uninstall`.
2. **[`verify_ioctl_abi.py`](../open-r580-driver/verify_ioctl_abi.py)**:
   - Verifies `NV_IOCTL_MAGIC = 'F'`, `NV_ESC_*`, `DRM_IOCTL_NVIDIA_*`, and live `/dev/nvidia*` nodes.
3. **[`p4_cuda_nvenc_smoke.c`](../open-r580-driver/p4_cuda_nvenc_smoke.c)** & **[`p4_nvdec_smoke.c`](../open-r580-driver/p4_nvdec_smoke.c)**:
   - Verified `CUDA[0]: Tesla P4 (8109 MiB)`, `VRAM DMA check: 5a 5a 5a 5a`, `NVENC API v13.0: session active, codec GUID count=2` (H.264 & HEVC), and `cuvidGetDecoderCaps` hardware decode for `H.264`, `HEVC` 8/10-bit, `VP9`, `MPEG2`, `JPEG`.

---

## 4. NVIDIA Container Toolkit + Selkies & LinuxServer.io PixelFlux (`10-bit` & `4:4:4` Color Depth)

Both `open-580-driver` (bare-metal R580) and `open-vgpu-tesla-p4` (KVM vGPU `535.309.01` + R620 Wayland/GBM overlay) are configured for **NVIDIA Container Toolkit (`1.20.2`)** and **LinuxServer.io Selkies (`2.2.1`) + PixelFlux (`2.2.3`)** with **10-bit and 4:4:4 (`YUV444`) hardware acceleration**:

### Hardware Capabilities Reported by `pixelflux` on Tesla P4 / `GRID P4-4Q`
```text
[pixelflux] Render node 0 encodes H264, H265 on nvenc.
encoders:  {'h264': 'nvenc', 'h265': 'nvenc'}
fullcolor: ['h264', 'h265']
formats:   {'h264': ['420-8', '444-8'], 'h265': ['420-8', '420-10', '444-8', '444-10']}
```

### Required Environment & Container Settings ([`docker-compose.yml`](../docker-compose.yml))
- `SELKIES_ENCODER=h265enc,h264enc` — Selects NVENC H.265/HEVC primary (which supports hardware **10-bit `4:4:4` (`444-10`)** and **10-bit `4:2:0` (`420-10`)** on Pascal GP104), with automatic fallback to NVENC H.264 **8-bit `4:4:4` (`444-8`)** for clients without HEVC decode.
- `SELKIES_VIDEO_FULLCOLOR=true` — Enables **4:4:4 (`YUV444` / `I444`)** chroma subsampling on NVENC.
- `SELKIES_VIDEO_10BIT=true` — Enables **10-bit (`10 bits per sample`)** color depth on NVENC H.265.
- `GBM_BACKENDS_PATH=/usr/lib64/gbm:/usr/lib/x86_64-linux-gnu/gbm` — Ensures Ubuntu/Debian containers locate `nvidia-drm_gbm.so -> libnvidia-allocator.so.1` on both Debian/Ubuntu (`/usr/lib/x86_64-linux-gnu/gbm`) and RHEL/AlmaLinux (`/usr/lib64/gbm`) hosts.

### Verified Live PixelFlux NVENC 10-bit 4:4:4 & 8-bit 4:4:4 Telemetry
```text
[NVENC] H265 initialized (4:4:4 mode: true, chroma convert: kernel).
[X11] Encoder: NVENC H265 4:4:4 10-bit on GRID P4-4Q (render node 0, nvidia driver), split as the driver picks, 2 engines.
[X11] Zero-copy capture (DRI3): 1024x768 blitted by the X server into 3 GPU buffers on /dev/dri/renderD128, encoded in place on NVENC.
[X11] Stream settings active -> Res: 1024x768 | FPS: 60.0 | Stripes: 1 | Mode: H265 (NVENC) FullFrame Streaming | CRF: 25 | Colorspace: I444 (Limited Range)
[NVENC] dmabuf imported as a CUDA-array frame (1 planes, 1024x768, pitch 0, 4 channels of element format 1): encoding in place.
```
