# 05 — Future Ubuntu Kernel Longevity & Tesla P4 Remote Desktop / Container Architecture

## 1. Focused Two-Stack Strategy (`open-580-driver` + `open-vgpu-tesla-p4`)

| Stack | Target Workload | Kernel Coupling & Stability | GPU Capabilities Unlocked |
| :--- | :--- | :--- | :--- |
| **[`open-r580-driver/`](../open-r580-driver/) + [`dkms-compat/`](../dkms-compat/)** | **Bare-Metal Linux (`580.178.04`), Wayland Desktop & Selkies/PixelFlux Containers** | Compiles cleanly on **Ubuntu 26.04 `7.0.0-43-generic`** via [`open_r580_dkms_builder.sh`](../open-r580-driver/open_r580_dkms_builder.sh) (`nvidia-drm` MIT, `nvidia-uvm` Dual MIT/GPL, `softdep drm_ttm_helper`) | **`8.0 GT/s` PCIe 3.0 x16**, **CUDA 13.0 (`8,109 MiB` VRAM DMA)**, **`NVENC 13.0` (`H.264`/`HEVC`)**, **`NVDEC`**, Wayland EGL (`1.1.20` / `wayland2 1.0.2`), and **Selkies + PixelFlux `10-bit` & `4:4:4` (`YUV444`)** |
| **[`open-vgpu-tesla-p4`](https://github.com/SanjayRavindran/open-vgpu-tesla-p4)** | **KVM Multi-VM vGPU (`580.196.09` Host + 50/50 `GRID P4-4Q` R580 Guests: Pure Wayland `580.196.07` & Windows 11 `582.99`)** | Host `580.196.09-vgpu-kvm` (`0xc85b7` bypass + `vgpuConfig.xml` `0x11D8`) on UEFI Coreboot (`192.168.1.24`, `7.0.0-44-generic`) + `/opt/nvidia-vgpu-580/` open daemons; Linux guest `580.196.07` (`nv-pci.c` `10de:1bb3` 2-line open-C patch, `-m=kernel`) | **2× `GRID P4-4Q` (`4,096 MiB` each)**: X11-stripped Pure Wayland GNOME (`--no-x11`) + CUDA `13.0` + NVENC `13.0` + Selkies CDI `10-bit 4:4:4` on AlmaLinux 10.3 (`192.168.1.68`), and `582.99` WHQL WDDM `3.1` + CUDA `13.0` + AVC444 RDP on Windows 11 24H2 (`192.168.1.74` DHCP / `.65`) |

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
   - Global `GSK_RENDERER=ngl` (`/etc/environment`, `/etc/systemd/user.conf.d/gsk-renderer.conf`, `org.gnome.Shell@wayland.service.d/override.conf`) switches GTK4 applications (`ptyxis`, `nautilus`, `gnome-control-center`) to hardware-accelerated OpenGL 4.6 / EGL on `GRID P4-4Q` while X11 artifacts (`nvidia-settings`, `nvidia-xconfig`, Xorg modules) are stripped cleanly.
2. **Windows 11 24H2 — WDDM `3.1` (`582.99` WHQL)**:
   - Runs on `GRID P4-4Q` (`4,096 MiB` GDDR5, `SUBSYS_11D810DE`) licensed via `fastapi-dls` (`https://192.168.1.24:443`) + `gridd-unlock-patcher` (`nvxdapix.dll`) as `NVIDIA RTX Virtual Workstation — Licensed` (90-day rotating leases) for DirectX 11/12 (`FL 12_1`), OpenGL `4.6`, Vulkan `1.3`, OpenCL, DirectCompute, DirectML, **CUDA `13.0`**, NVENC, NVDEC, TechPowerUp GPU-Z, and NVIDIA Control Panel.
   - Uses `1920 × 1080` (`dynamic resolution:i:0`, `smart sizing:i:1`) with NLA (`UserAuthentication=1`) and without `AVCHardwareEncodePreferred` policy overrides to prevent `RDP_GFX 0x8007007A` (`Error code: 0x200d`) buffer overflows on high-DPI Retina clients (`3600 × 2262`).
3. **Selkies + LinuxServer.io PixelFlux Container (`10-bit` & `4:4:4` Color Depth)**:
   - Powered by Docker + NVIDIA Container Toolkit (`nvidia-ctk config --in-place --set nvidia-container-runtime.mode=cdi`, with `.note.gnu.property` `x86-64-v3` stripped on `el10` for `x86-64-v2` Ivy Bridge Xeon compatibility) with `SELKIES_ENCODER=h265enc,h264enc`, `SELKIES_VIDEO_FULLCOLOR=true`, and `SELKIES_VIDEO_10BIT=true`.
   - Streams zero-copy DMA-BUF (`DRI3` -> `CUDA-array` -> `NVENC`) in **`NVENC H265 4:4:4 10-bit` (`444-10`)** and **`NVENC H264 4:4:4 8-bit` (`444-8`)**.
4. **Host Delegated License Service (`fastapi-dls`)**:
   - Deployed on the KVM host at `https://192.168.1.24:443` (`/opt/fastapi-dls/docker-compose.yml`, `collinwebdesigns/fastapi-dls:latest` v2.0.3), serving client configuration tokens at `https://192.168.1.24/-/client-token`, root CA at `https://192.168.1.24/-/config/root-certificate`, and active lease management at `https://192.168.1.24/-/origins?leases=true`.


---

## Companion vGPU Stack Update (`open-vgpu-tesla-p4`)

For the 50/50 Tesla P4 `GRID P4-4Q` R580 dual-VM stack (`vm-alma` + `vm-win11` on `192.168.1.24`), see [`SanjayRavindran/open-vgpu-tesla-p4`](https://github.com/SanjayRavindran/open-vgpu-tesla-p4):
- Clean DKMS `nvidia-vgpu-kvm/580.196.09` packaging (`0xc85b7` bypass + `patch_r580_vgpu_config_xml.py` `SUBSYS_11D8`) + open-source `/opt/nvidia-vgpu-580/` daemons (`open-nvidia-vgpud`, `libopen-vgxcfg.so`, `open-nvidia-xid-logd`, `open-nvidia-vgpu-mgr` with R580 `0x98` B `SET_VM_NAME`, `open-nvidia-smi` + `libopen-nvidia-ml.so`).
- Streamlined **Ubuntu KVM**, **Proxmox VE 8/9**, multi-distro Linux (`580.196.07` `nv-pci.c` `10de:1bb3` patch + X11 strip + CDI), and Windows 11 (`582.99` WHQL + `nvxdapix.dll` patch) automation scripts.
