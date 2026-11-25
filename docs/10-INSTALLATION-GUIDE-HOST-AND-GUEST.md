# Complete Host & Guest Installation Guide (`open-580-driver` & `open-vgpu-tesla-p4`)

This guide provides step-by-step instructions to install either the **Bare-Metal / PCIe-Passthrough `open-580-driver` (`580.178.04`)** stack or the **50/50 Multi-VM `open-vgpu-tesla-p4` (`580.196.09-vgpu-kvm` + 11 Clean-Room Open-Source Host & Guest Components)** stack across **Host** and **Guest** machines.

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

## 2. Track B — `open-vgpu-tesla-p4` (`580.196.09-vgpu-kvm` 50/50 Multi-VM Host & Guests)

Use this track to partition a single Tesla P4 into **two concurrent `GRID P4-4Q` (`nvidia-65`, `4096 MiB` VRAM each) vGPUs** on **Ubuntu 24.04/26.04 KVM** or **Proxmox VE 8/9**, running **all 11 clean-room open-source host & guest components**:
1. **`open-nvidia-vgpud`** (`Rust`, `nova-vgpu/`) — RM `NVA081` profile registrar
2. **`libopen-vgxcfg.so`** (`C`, `open-vgxcfg/`) — XML DOM configuration library (`9/9` exports)
3. **`open-nvidia-xid-logd`** (`C`, `open-xid-logd/`) — NVML XID event & Base64 debugdump daemon
4. **`open-nvidia-vgpu-mgr`** (`C`, `open-vgpu-mgr/`) — KVM/VFIO `vmiop-env` runtime daemon (`60/60` `vmiop_*` exports)
5. **`open-nvidia-smi`** (`C`, `open-nvml-smi/`) — Clean-room `nvidia-smi` CLI (89-column table, `-L`, `-q`, `vgpu -s -c -q`, `--query-gpu` CSV)
6. **`libopen-nvidia-ml.so`** (`C`, `open-nvml-smi/`) — Clean-room NVML shared library (`libnvidia-ml.so.1` SONAME)
7. **`libopen-nvidia-vgpu.so`** (`C`, `open-vgpu-plugin/`) — Clean-room `vmiop` mediator plugin (`VER_5_0`/`VER_2_0`, `47/47` VRPCs, `0x50`/`0x9c` static info + hybrid interposer)
8. **`open-vgpu-guest` & `nova_vgpu_guest.ko`** (`C`, `open-vgpu-guest/`, `kmods/nova_vgpu_guest/`) — Clean-room guest BAR0 `0x8f000` `"VRPC"` client & zero-blob `open-gpu-kernel-modules` `nvidia*.ko` overlay
9. **`libopen-nvidia-encode.so`** (`C`, `open-nvenc/`) — Standalone clean-room NVENC `13.0` (`0xd0`) userspace library (`libnvidia-encode.so.1` SONAME, `libc.so.6` only, `10/10` exports, `43/43` function slots, GP104 `NVC2B7` `0xc2b7` + `NVC06F` + `NVC0B5` H.264/HEVC 8/10-bit 4:2:0 & 4:4:4 + cached BL16 swizzle, `diff-hw-test` verified)
10. **`libopen-host-rm-gp104.so`** (`C`, `open-host-rm/`) — Clean-room Tier 2B host GP104 physical RM HAL slice (`8/8` Category #1 signed Falcon WPR1/ACR LSF headers, `22` vGPU profiles, 50/50 `GRID P4-4Q` split)
11. **`open-gridd`** (`C11`, `open-gridd/`) — Clean-room Linux NLS/DLS licensing daemon (`/usr/bin/nvidia-gridd`, OpenSSL 3.x), replacing the `5.31 MiB` vendor `nvidia-gridd` binary and verified live on `vm-alma` against FastAPI-DLS (`https://192.168.1.24:443`)

### 2A. Hypervisor Host Installation (`--role host` on Ubuntu KVM or Proxmox VE 8/9)
```bash
# Dry-run check
bash scripts/install_open_vgpu.sh \
  --role host \
  --run-file /path/to/NVIDIA-Linux-x86_64-580.196.09-vgpu-kvm.run \
  --with-dls docker \
  --dls-url 192.168.1.24 \
  --dry-run

# Full Host Install (Ubuntu KVM with Docker FastAPI-DLS)
sudo bash scripts/install_open_vgpu.sh \
  --role host \
  --run-file /path/to/NVIDIA-Linux-x86_64-580.196.09-vgpu-kvm.run \
  --with-dls docker \
  --dls-url 192.168.1.24

# Full Host Install (Proxmox VE 8/9 one-shot host configurator: IOMMU + headers + DKMS + open daemons + systemd FastAPI-DLS + VMs)
sudo bash scripts/install_proxmox_vgpu_host.sh \
  --run-file /path/to/NVIDIA-Linux-x86_64-580.196.09-vgpu-kvm.run \
  --enable-no-subscription-repo \
  --dls-url <PROXMOX_HOST_IP> \
  --setup-vms fresh

# Or via unified installer on Proxmox VE 8/9:
sudo bash scripts/install_open_vgpu.sh \
  --role host \
  --run-file /path/to/NVIDIA-Linux-x86_64-580.196.09-vgpu-kvm.run \
  --with-dls systemd \
  --dls-url <PROXMOX_HOST_IP>
```

Verify on the host after installation:
```bash
dkms status
systemctl status nvidia-vgpud nvidia-vgpu-mgr nvidia-xid-logd
/opt/nvidia-vgpu-580/open-nvidia-smi
/opt/nvidia-vgpu-580/open-nvidia-smi vgpu -s
```

### 2B. Streamlined Multi-Distro Linux vGPU Guest (`580.196.07` + Pure Wayland X11 Strip + CDI)
On `192.168.1.24`, the R580 guest installers (`NVIDIA-Linux-x86_64-580.196.07-grid.run`, `.rpm`, `.deb`) and `gridd-unlock-patcher` are staged under `/home/zero/guest/`. Native R580 provides Wayland/GBM EGL and **CUDA `13.0`** directly (no R620 overlay or GRD PTX downgrade required). Run the streamlined provisioner (patches `kernel/nvidia/nv-pci.c` for `10de:1bb3`, builds `-m=kernel` DKMS, strips NVIDIA X11 files, patches `nvidia-gridd`, and configures Docker CDI `mode = "cdi"`):
```bash
sudo bash scripts/provision_linux_vgpu_guest.sh \
  --dls-url https://192.168.1.24 \
  --grid-580-run /path/to/NVIDIA-Linux-x86_64-580.196.07-grid.run \
  --gridd-patcher /path/to/gridd-unlock-patcher \
  --rdp-user zero \
  --rdp-pass '<YOUR_RDP_PASSWORD>' \
  --with-selkies
```

### 2C. Windows 11 24H2 vGPU Guest Installation (`--role guest-windows`, `582.99` WHQL)
1. For unattended Windows 11 24H2 ISO installation (with WinRM NTLM and Remote Desktop pre-enabled), attach `scripts/templates/win11-autounattend.xml` (replacing `__ADMIN_PASSWORD__`).
2. Extract & patch `Display.Driver/nvxdapix.dll` (`9594600` B) on the host via `7z` + `gridd-unlock-patcher` (with `https://192.168.1.24/-/config/root-certificate`), copy `582.99...exe` and `nvxdapix_patched.dll` into the VM, and run in elevated PowerShell:
   ```powershell
   powershell -ExecutionPolicy Bypass -File scripts\provision_win11_guest.ps1 `
     -DlsUrl https://192.168.1.24 `
     -DriverInstallerPath C:\path\to\582.99_grid_win10_win11_server2022_server_2025_dch_64bit_international.exe `
     -PatchedNvxdapixPath C:\path\to\nvxdapix_patched.dll
   ```
   This binds `582.99` WHQL WDDM `3.1` (`SUBSYS_11D810DE` -> `GRID P4-4Q`), installs the patched `nvxdapix.dll` into the matching `582.99` DriverStore folder, downloads the FastAPI-DLS token (`FeatureType=1`), disables `Microsoft Basic Display Adapter`, removes RDP `0x200d` policy overrides, and enables NLA (`UserAuthentication=1`).
