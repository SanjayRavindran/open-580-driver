# 05 — Future Ubuntu Kernel Longevity & Tesla P4 Remote Desktop / Container Architecture

## 1. Focused Two-Stack Strategy (`open-580-driver` + `open-vgpu-tesla-p4`)

| Stack | Target Workload | Kernel Coupling & Stability | GPU Capabilities Unlocked |
| :--- | :--- | :--- | :--- |
| **[`open-r580-driver/`](../open-r580-driver/) + [`dkms-compat/`](../dkms-compat/)** | **Bare-Metal Linux (`580.178.04`), Wayland Desktop & Selkies/PixelFlux Containers** | Compiles cleanly on **Ubuntu 26.04 `7.0.0-43-generic`** via [`open_r580_dkms_builder.sh`](../open-r580-driver/open_r580_dkms_builder.sh) (`nvidia-drm` MIT, `nvidia-uvm` Dual MIT/GPL, `softdep drm_ttm_helper`) | **`8.0 GT/s` PCIe 3.0 x16**, **CUDA 13.0 (`8,109 MiB` VRAM DMA)**, **`NVENC 13.0` (`H.264`/`HEVC`)**, **`NVDEC`**, Wayland EGL (`1.1.20` / `wayland2 1.0.2`), and **Selkies + PixelFlux `10-bit` & `4:4:4` (`YUV444`)** |
| **[`open-vgpu-tesla-p4`](https://github.com/SanjayRavindran/open-vgpu-tesla-p4)** | **KVM Multi-VM vGPU (`535.309.01` Host + 50/50 `GRID P4-4Q` Guests: Pure Wayland Linux & Windows 11 WDDM `539.72`)** | Host `535.309.01-vgpu-kvm` on UEFI Coreboot (`192.168.1.24`, `7.0.0-43-generic`) + clean-room `nova-vgpu` Rust/kernel mediator & R620 Wayland EGL/GBM overlay | **2× `GRID P4-4Q` (`4,096 MiB` each)**: Pure Wayland GNOME (`--no-x11`) + hardware AVC444 RDP + Selkies/PixelFlux `10-bit 4:4:4` on AlmaLinux 10.3 (`192.168.1.68`), and WDDM `3.1` + D3D11/12 + GPU-Z + NVIDIA Control Panel + hardware AVC444 RDP on Windows 11 24H2 (`192.168.1.65`) |

> [!NOTE]
> The experimental pure `nova-core` / Mesa `NVK` / Zink track has been retired so maintenance stays 100% focused on the full-performance `open-580-driver` and `open-vgpu-tesla-p4` stacks with official CUDA, NVENC, NVDEC, Pure Wayland (Linux), and WDDM (Windows) acceleration.

## 2. Why `580.178.04` Survives Future Kernels

Direct Kbuild testing of `580.178.04/kernel/` on **`Linux 7.0.0-43-generic`** compiles cleanly (`exit 0`) into all required `.ko` modules ([`r580-linux-7x-compat-matrix.md`](../dkms-compat/r580-linux-7x-compat-matrix.md)). Because the closed core (`nv-kernel.o_binary`) speaks strictly to `common/inc/{nv-kernel-interface-api.h,os-interface.h}` implemented by `nv-interface.o` (`MIT`), upgrading Ubuntu across future `7.x` kernels only requires maintaining small open C adapters around:
- DRM KMS/FB structs (`nvidia-drm/nvidia-drm-drv.c`, `nvidia-drm-fb.c`)
- Timer & MM symbols (`hrtimer_setup`, `timer_delete_sync`, `follow_pfnmap_start`, `make_device_exclusive`)
- Kernel upgrade guard: run [`verify_r580_kbuild.sh`](../dkms-compat/verify_r580_kbuild.sh) against newly installed `linux-headers-*` before rebooting into a new kernel.

## 3. Recommended Tesla P4 Architecture for Wayland Desktop, WDDM & Selkies/PixelFlux Containers

1. **Linux Desktop — Pure Wayland (`gnome-shell --no-x11 --headless`)**:
   - Uses `nvidia-drm modeset=1` paired with `libnvidia-egl-wayland.so.1.1.20`, `libnvidia-egl-wayland2.so.1.0.2` (`09_nvidia_wayland2.json`), and `libnvidia-egl-gbm.so.1.1.3` (`15_nvidia_gbm.json`).
   - Hardware-accelerated remote desktop via `gnome-remote-desktop-headless.service` (`--headless` user daemon on `:3389` with `RDPGFX_CAPVERSION_107`, CUDA + NVENC H.264 `AVC444`), avoiding `--system` Handover Server Redirection NLA MIC (`0x8009030f`) failures on macOS RDP clients.
   - Global `GSK_RENDERER=ngl` (`/etc/environment`, `/etc/systemd/user.conf.d/gsk-renderer.conf`, `org.gnome.Shell@wayland.service.d/override.conf`) switches GTK4 applications (`ptyxis`, `nautilus`, `gnome-control-center`) from the `vulkan` GDK renderer (`libnvidia-vulkan-producer.so` swapchain segfault in 535) to hardware-accelerated OpenGL 4.6 / EGL on `GRID P4-4Q`.
2. **Windows 11 24H2 — WDDM `3.1` (`539.72` WHQL)**:
   - Runs on `GRID P4-4Q` (`4,096 MiB` GDDR5, `SUBSYS_120610DE`) licensed via `fastapi-dls` (`https://192.168.1.24:443`) as `NVIDIA RTX Virtual Workstation — Licensed` (90-day rotating leases) for DirectX 11/12 (`FL 12_1`), OpenGL `4.6`, Vulkan `1.3`, OpenCL, DirectCompute, DirectML, CUDA `12.2`, NVENC `12.1`, NVDEC, TechPowerUp GPU-Z `2.71.0`, and NVIDIA Control Panel (`8.1.969.0`).
   - Uses `1920 × 1080` (`dynamic resolution:i:0`, `smart sizing:i:1`) with NLA (`UserAuthentication=1`) and without `AVCHardwareEncodePreferred` policy overrides to prevent `RDP_GFX 0x8007007A` (`Error code: 0x200d`) buffer overflows on high-DPI Retina clients (`3600 × 2262`).
3. **Selkies + LinuxServer.io PixelFlux Container (`10-bit` & `4:4:4` Color Depth)**:
   - Powered by Docker + NVIDIA Container Toolkit (`nvidia-ctk` CDI v0.7.0, with `.note.gnu.property` `x86-64-v3` stripped on `el10` for `x86-64-v2` Ivy Bridge Xeon compatibility) with `SELKIES_ENCODER=h265enc,h264enc`, `SELKIES_VIDEO_FULLCOLOR=true`, and `SELKIES_VIDEO_10BIT=true`.
   - Streams zero-copy DMA-BUF (`DRI3` -> `CUDA-array` -> `NVENC`) in **`NVENC H265 4:4:4 10-bit` (`444-10`)** and **`NVENC H264 4:4:4 8-bit` (`444-8`)**.
4. **Host Delegated License Service (`fastapi-dls`)**:
   - Deployed on the KVM host at `https://192.168.1.24:443` (`/opt/docker/fastapi-dls/docker-compose.yml`, `collinwebdesigns/fastapi-dls:latest` v2.0.3), serving client configuration tokens at `https://192.168.1.24/-/client-token` and active lease management at `https://192.168.1.24/-/origins?leases=true`.


---

## Companion vGPU Stack Update (`open-vgpu-tesla-p4`, 2026-11-21)

For the 50/50 Tesla P4 `GRID P4-4Q` dual-VM stack (`vm-alma` + `vm-win11` on `192.168.1.24`), see [`SanjayRavindran/open-vgpu-tesla-p4`](https://github.com/SanjayRavindran/open-vgpu-tesla-p4):
- Clean DKMS `nvidia-vgpu-kvm/535.309.01` packaging + open-source `open-nvidia-vgpud`, `libopen-vgxcfg.so`, and `open-nvidia-xid-logd` (`rea@6.10.1` verified).
- Full **Ubuntu KVM** and **Proxmox VE 8/9** hypervisor & guest automation scripts (`install_vgpu_host_dkms.sh`, `install_fastapi_dls.sh`, `setup_proxmox_vgpu_vms.sh`, `provision_alma_guest.sh`, `provision_win11_guest.ps1`).
