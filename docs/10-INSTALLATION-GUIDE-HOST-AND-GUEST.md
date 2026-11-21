# Complete Host & Guest Installation Guide (`open-580-driver` & `open-vgpu-tesla-p4`)

This guide provides step-by-step instructions to install either the **Bare-Metal / PCIe-Passthrough `open-580-driver` (`580.178.04`)** stack or the **50/50 Multi-VM `open-vgpu-tesla-p4` (`535.309.01-vgpu-kvm` + 6 Clean-Room Open-Source Host Binaries)** stack across **Host** and **Guest** machines.

---

## 1. Track A — `open-580-driver` (`580.178.04` Bare-Metal Host or PCIe-Passthrough Linux Guest)

Use this track when assigning the entire physical **Tesla P4 (`GP104GL`, `10de:1bb3`)** to a single bare-metal Linux host (`Linux 6.x / 7.0+`) or a single full PCIe-passthrough (`vfio-pci`) Linux VM for **CUDA 13.0**, **NVENC 13.0**, **NVDEC**, **Wayland EGL/GBM**, and **Selkies / PixelFlux 10-bit 4:4:4 (`YUV444`)**.

### 1A. Bare-Metal Host Installation (`--role host`)
```bash
# Dry-run verification first (no root required)
bash scripts/install_open_580.sh --role host --download --dry-run

# Full DKMS + non-clobbering userspace + Wayland/GBM + CDI + CUDA/NVENC/NVDEC smoke tests
sudo bash scripts/install_open_580.sh \
  --role host \
  --run-file /path/to/NVIDIA-Linux-x86_64-580.178.04.run \
  --with-selkies
```
*(Or pass `--download` instead of `--run-file` to download `580.178.04` automatically via `open-r580-driver/extract_r580_binaries.sh`.)*

### 1B. PCIe-Passthrough Linux Guest Installation (`--role guest-linux`)
When the Tesla P4 is passed through as a whole PCIe device (`01:00.0`) to a Linux VM:
```bash
sudo bash scripts/install_open_580.sh \
  --role guest-linux \
  --run-file /path/to/NVIDIA-Linux-x86_64-580.178.04.run \
  --with-selkies
```

### 1C. Clean Uninstall of `open-580-driver`
Before switching a host from bare-metal `open-580-driver` to KVM vGPU (`open-vgpu-tesla-p4`):
```bash
sudo bash scripts/install_open_580.sh --uninstall
```

---

## 2. Track B — `open-vgpu-tesla-p4` (`535.309.01-vgpu-kvm` 50/50 Multi-VM Host & Guests)

Use this track to partition a single Tesla P4 into **two concurrent `GRID P4-4Q` (`nvidia-65`, `4096 MiB` VRAM each) vGPUs** on **Ubuntu 24.04/26.04 KVM** or **Proxmox VE 8/9**, running **all 6 clean-room open-source host userspace binaries**:
1. **`open-nvidia-vgpud`** (`Rust`, `nova-vgpu/`) — RM `NVA081` profile registrar
2. **`libopen-vgxcfg.so`** (`C`, `open-vgxcfg/`) — XML DOM configuration library (`9/9` exports)
3. **`open-nvidia-xid-logd`** (`C`, `open-xid-logd/`) — NVML XID event & Base64 debugdump daemon
4. **`open-nvidia-vgpu-mgr`** (`C`, `open-vgpu-mgr/`) — KVM/VFIO `vmiop-env` runtime daemon (`60/60` `vmiop_*` exports)
5. **`open-nvidia-smi`** (`C`, `open-nvml-smi/`) — Clean-room `nvidia-smi` CLI (89-column table, `-L`, `-q`, `vgpu -s -c -q`, `--query-gpu` CSV)
6. **`libopen-nvidia-ml.so`** (`C`, `open-nvml-smi/`) — Clean-room NVML shared library (`libnvidia-ml.so.1` SONAME)

### 2A. Hypervisor Host Installation (`--role host` on Ubuntu KVM or Proxmox VE 8/9)
```bash
# Dry-run check
bash scripts/install_open_vgpu.sh \
  --role host \
  --run-file /path/to/NVIDIA-Linux-x86_64-535.309.01-vgpu-kvm.run \
  --with-dls docker \
  --dls-url 192.168.1.24 \
  --dry-run

# Full Host Install (Ubuntu KVM with Docker FastAPI-DLS)
sudo bash scripts/install_open_vgpu.sh \
  --role host \
  --run-file /path/to/NVIDIA-Linux-x86_64-535.309.01-vgpu-kvm.run \
  --with-dls docker \
  --dls-url 192.168.1.24

# Full Host Install (Proxmox VE 8/9 with native systemd FastAPI-DLS)
sudo bash scripts/install_open_vgpu.sh \
  --role host \
  --run-file /path/to/NVIDIA-Linux-x86_64-535.309.01-vgpu-kvm.run \
  --with-dls systemd \
  --dls-url <PROXMOX_HOST_IP>
```

Verify on the host after installation:
```bash
dkms status
systemctl status nvidia-vgpud nvidia-vgpu-mgr nvidia-xid-logd
/opt/nvidia-vgpu-535/open-nvidia-smi
/opt/nvidia-vgpu-535/open-nvidia-smi vgpu -s
```

### 2B. Linux vGPU Guest Installation (`--role guest-linux` on AlmaLinux 10.3 / RHEL 10 / Ubuntu)
Installs `535.309.01` GRID kernel modules, overlays `620.70.06` Wayland/GBM userspace libraries (`libnvidia-egl-gbm`, `libnvidia-egl-wayland`, `libnvidia-egl-wayland2`, `nvidia-drm_gbm.so`), configures pure Wayland headless GNOME (`--no-x11 --headless`) + `GSK_RENDERER=ngl` + GNOME Remote Desktop CUDA PTX `8.0` patch (`:3389`), fetches the FastAPI-DLS client token, and optionally installs Docker CE + NVIDIA CDI + Selkies PixelFlux 10-bit 4:4:4:
```bash
sudo bash scripts/install_open_vgpu.sh \
  --role guest-linux \
  --dls-url https://192.168.1.24 \
  --grid-535-run /path/to/NVIDIA-Linux-x86_64-535.309.01-grid.run \
  --grid-620-run /path/to/NVIDIA-Linux-x86_64-620.70.06-grid.run \
  --rdp-user zero \
  --rdp-pass '<YOUR_RDP_PASSWORD>' \
  --with-selkies
```

### 2C. Windows 11 24H2 vGPU Guest Installation (`--role guest-windows`)
1. For unattended Windows 11 24H2 ISO installation (with WinRM NTLM and Remote Desktop pre-enabled), attach `scripts/templates/win11-autounattend.xml` (replacing `__ADMIN_PASSWORD__`).
2. Inside an elevated PowerShell prompt on the Windows 11 VM, run:
   ```powershell
   powershell -ExecutionPolicy Bypass -File scripts\provision_win11_guest.ps1 `
     -DlsUrl https://192.168.1.24 `
     -DriverInstallerPath C:\path\to\539.72_grid_win10_win11_64bit_international.exe
   ```
   This binds the `539.72` WHQL WDDM 3.1 driver (`SUBSYS_120610DE` -> `GRID P4-4Q`), downloads the FastAPI-DLS client configuration token, sets `FeatureType=1`, removes `AVCHardwareEncodePreferred` / `AVC444ModePreferred` policy overrides that cause RDP Error `0x200d` (`RDP_GFX 0x8007007A`), and enables NLA (`UserAuthentication=1`).
