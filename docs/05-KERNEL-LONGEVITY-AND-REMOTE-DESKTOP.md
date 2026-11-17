# 05 — Future Ubuntu Kernel Longevity & Tesla P4 Remote Desktop Architecture

## 1. Three-Track Strategy to Beat Planned Obsolescence

| Track | Timescale | Kernel Coupling & Stability | GPU Capabilities Unlocked | Maintenance Burden |
| :--- | :--- | :--- | :--- | :--- |
| **Track A (`open-r580-driver/` + `dkms-compat/`)** | **Now -> Linux `7.x`** | Already compiles cleanly out-of-the-box on **Ubuntu 26.04 `7.0.0-43-generic`** via [`open_r580_dkms_builder.sh`](../open-r580-driver/open_r580_dkms_builder.sh) (`nvidia-drm` MIT, `nvidia-uvm` Dual MIT/GPL, `softdep drm_ttm_helper`) | **100% hardware potential (live-verified)**: **`8.0 GT/s` PCIe 3.0 x16**, **CUDA 13.0 (`8,109 MiB` VRAM DMA)**, and **`NVENC 13.0` (`2` codecs H.264/HEVC)** via [`07-OPEN-R580-HYBRID-AND-CUDA-NVENC.md`](./07-OPEN-R580-HYBRID-AND-CUDA-NVENC.md) | Patch ~5–15 small `conftest` / DRM / MM glue lines per major upstream kernel release using [verify_r580_kbuild.sh](file:///c:/Users/Sanjay/Documents/AI%20Projects/p4/dkms-compat/verify_r580_kbuild.sh) |
| **Track B: Upstream `nouveau` + Mesa (`NVK`/Zink)** | **Forever (Upstream Linux)** | Zero out-of-tree modules; survives every Ubuntu kernel upgrade automatically | Vulkan 1.3+ (`NVK`) & OpenGL via Zink (`renderD129`), **but** limited by missing `/lib/firmware/nvidia/gp104/pmu/` (`2.5 GT/s` idle clocks) and unimplemented `nvkm/engine/nvenc` stub | Wire extracted `pmu` blobs (`0x2dac0` + `300,288B` RTOS) from [`firmware-blobs/gp104/pmu/`](../firmware-blobs/gp104/pmu/) |
| **Track C: Clean-Room `nova-core` Pascal (`nova-pascal`)** | **Long-Term Research** | Rust in-kernel DRM driver (`CONFIG_RUST=y` enabled on Ubuntu 26.04) | Stages `WPR1`, `PMU`, `SEC2`, `NVENC`, `FECS`, `GPCCS`, and `NVDEC` using isolated blobs + open Rust host control (**open UMD path; not drop-in `libcuda.so`/`libnvidia-encode.so`**) | Highest (~full host MMU/FIFO/GR/PMU/NVENC engine stack required in Rust) |

## 2. Why `580.178.04` Survives Future Kernels Better Than Expected

Our direct Kbuild test of `580.178.04/kernel/` on **`Linux 7.0.0-43-generic`** compiled cleanly (`exit 0`) into all five `.ko` modules ([r580-linux-7x-compat-matrix.md](file:///c:/Users/Sanjay/Documents/AI%20Projects/p4/dkms-compat/r580-linux-7x-compat-matrix.md)). Because the `112.68 MB` closed blob (`nv-kernel.o_binary`) speaks strictly to `common/inc/{nv-kernel-interface-api.h,os-interface.h}` implemented by `nv-interface.o` (`MIT`), upgrading Ubuntu across `7.1`, `7.2`, etc. only requires maintaining tiny open C adapters around:
- DRM KMS/FB structs (`nvidia-drm/nvidia-drm-drv.c`, `nvidia-drm-fb.c` — e.g. Linux 6.12 `output_poll_changed` removal and 6.17 `fb_create` `drm_format_info` parameter detected via `216` probes under `kernel/conftest/`)
- Timer & MM symbols (`hrtimer_setup`, `timer_delete_sync`, `follow_pfnmap_start`, `make_device_exclusive`)
- Kernel upgrade guard: always run [verify_r580_kbuild.sh](file:///c:/Users/Sanjay/Documents/AI%20Projects/p4/dkms-compat/verify_r580_kbuild.sh) against newly installed `linux-headers-*` via an APT/DKMS test step before rebooting into a new kernel.

### Realistic End-of-Life Warning for `nv-kernel.o_binary`

The eventual hard limit of Track A is not C API churn—it is x86-64 binary-ABI enforcement in future Linux kernels (`objtool` validation rules for retpolines/FINEIBT/altstack frames, or converting remaining shim dependencies to `EXPORT_SYMBOL_GPL` blocked by `MODULE_LICENSE("NVIDIA")`). That is why archiving the **signed GP104 Falcon blobs** extracted by [nv_bindata_analyzer.py](file:///c:/Users/Sanjay/Documents/AI%20Projects/p4/tools/nv_bindata_analyzer.py) alongside Tracks B and C preserves long-term hardware independence.

## 3. Recommended Tesla P4 Architecture for Remote Desktop & Apps

On the inspected host, **Intel `i915` (`card1`/`renderD128`)** owns physical display connectors (`DP-1`, `HDMI-A-1`, `VGA-1`) while **Tesla P4 (`01:00.0`, `card0`/`renderD129`)** is a headless `0x030200` 3D controller with zero physical display ports. That dual-GPU split is ideal for high-performance remote desktops and virtualized apps:

1. **Driver Selection for Today (`Track A`)**:
   - Use **R580.178.04** (`nvidia`, `nvidia-uvm`, `nvidia-drm`) pinned for compute/offload rendering on `01:00.0`, leaving `i915` driving console/VT display so you avoid X11 modesetting conflicts on headless Tesla P4 (`nvidia-modeset.ko` is not required for primary scanout).
   - Enable `nvidia-persistenced` and `nvidia-smi -pm 1` so GPU stays initialized cleanly across remote sessions without cold-boot Falcon reloads.
2. **Low-Latency Remote Desktop / App Streaming**:
   - **GPU-accelerated desktop streaming (Sunshine + Moonlight / Parsec-style / PipeWire virtual monitor)**: Headless Xorg (`AllowEmptyInitialConfiguration`, dummy modeline) or wlroots/KWin headless compositor renders desktop & 3D apps on Tesla P4 while `libnvidia-encode.so.580.178.04` (`NvEncodeAPICreateInstance`) offloads H.264/HEVC encoding to GP104's dedicated NVENC silicon—yielding multi-stream 1080p/4K@60fps remote desktop at low CPU load.
   - **Per-app Vulkan / OpenGL / CUDA offload (`DRI_PRIME` / `__NV_PRIME_RENDER_OFFLOAD=1` / `VirtualGL` over `xrdp` / `TurboVNC`)**: Keep lightweight desktop compositing on `i915` or CPU `Xvnc`, and launch heavy 3D/CAD/AI/browser apps against Tesla P4 (`renderD129` / CUDA / `EGL` headless context).
3. **Pure Open-Source Fallback (`Track B` on Ultra-Future Kernels)**:
   - If a future Linux `8.x` kernel breaks proprietary `nvidia.ko` linkage altogether, `nouveau` + Mesa `NVK` + `Zink` guarantees uninterrupted Vulkan/OpenGL/3D desktop offload via `/dev/dri/renderD129` (`i915` or headless Wayland compositors encode frames in software/VA-API `i915`), while the extracted `gp104_pmu_rtos` (`300,288` bytes) + `gp104_acr_*` + [`nova-pascal`](file:///c:/Users/Sanjay/Documents/AI%20Projects/p4/nova-pascal/src/lib.rs) clean-room specification documents everything needed to unlock full PCIe `8.0 GT/s` clocks cleanly in open code.

