#!/usr/bin/env bash
set -euo pipefail

# Unified Host & Guest Installer for open-580-driver (NVIDIA 580.178.04 Open-Shim Hybrid Stack)
# Supports:
#   --role host         Bare-metal Linux 6.x/7.x host (DKMS kernel shims + verified .o_binary + userspace + Wayland/GBM + CDI + smoke tests)
#   --role guest-linux  PCIe passthrough (vfio-pci) Linux VM guest (DKMS kernel shims + userspace + Wayland/GBM + CDI + optional Selkies 10-bit 4:4:4)
#   --uninstall         Cleanly remove open-580-driver DKMS modules, modprobe/udev configs, and tracked userspace files

ROLE="host"
RUN_FILE=""
DRIVER_DIR=""
DOWNLOAD=0
WITH_USERSPACE=1
WITH_SELKIES=0
RUN_SMOKE=1
UNINSTALL=0
DRY_RUN=0

usage() {
  cat <<'EOF'
Usage: install_open_580.sh [--role host|guest-linux] [options]

Roles:
  --role host           Install open-580-driver (580.178.04) on a bare-metal Linux host (default)
  --role guest-linux    Install open-580-driver (580.178.04) inside a PCIe-passthrough Linux guest VM

Options:
  --run-file <path>     Path to NVIDIA-Linux-x86_64-580.178.04.run
  --driver-dir <path>   Path to pre-extracted NVIDIA-Linux-x86_64-580.178.04 directory
  --download            Download NVIDIA-Linux-x86_64-580.178.04.run automatically via extract_r580_binaries.sh
  --no-userspace        Install DKMS kernel modules only (skip userspace libraries)
  --with-selkies        Also start the Selkies + PixelFlux 10-bit 4:4:4 (YUV444) Docker Compose stack
  --skip-smoke          Skip compiling and running the CUDA 13.0 / NVENC 13.0 / NVDEC C smoke tests
  --uninstall           Uninstall open-580-driver DKMS modules and tracked userspace runtime
  --dry-run             Validate and print planned commands without modifying the system
  -h, --help            Show this help message
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --role)           ROLE="${2:-}"; shift 2 ;;
    --run-file)       RUN_FILE="${2:-}"; shift 2 ;;
    --driver-dir)     DRIVER_DIR="${2:-}"; shift 2 ;;
    --download)       DOWNLOAD=1; shift ;;
    --no-userspace)   WITH_USERSPACE=0; shift ;;
    --with-selkies)   WITH_SELKIES=1; shift ;;
    --skip-smoke)     RUN_SMOKE=0; shift ;;
    --uninstall)      UNINSTALL=1; shift ;;
    --dry-run)        DRY_RUN=1; shift ;;
    -h|--help)        usage; exit 0 ;;
    *)                echo "Unknown argument: $1" >&2; usage >&2; exit 2 ;;
  esac
done

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
R580_DIR="$REPO_ROOT/open-r580-driver"

if [[ "$ROLE" != "host" && "$ROLE" != "guest-linux" ]]; then
  echo "ERROR: --role must be 'host' or 'guest-linux' (got '$ROLE')." >&2
  exit 2
fi

if [[ "$UNINSTALL" -eq 1 ]]; then
  if [[ "$DRY_RUN" -eq 1 ]]; then
    echo "[dry-run] bash $R580_DIR/open_r580_dkms_builder.sh --uninstall"
    exit 0
  fi
  exec bash "$R580_DIR/open_r580_dkms_builder.sh" --uninstall
fi

echo "=== open-580-driver Installer (role=${ROLE}, userspace=${WITH_USERSPACE}, selkies=${WITH_SELKIES}, dry_run=${DRY_RUN}) ==="

if [[ "$DRY_RUN" -eq 1 ]]; then
  echo "[dry-run] 1. Ensure prerequisites: dkms build-essential linux-headers-$(uname -r) curl"
  if [[ "$DOWNLOAD" -eq 1 ]]; then
    echo "[dry-run] 2. bash $R580_DIR/extract_r580_binaries.sh --download --output /tmp/r580_pkg"
  fi
  echo "[dry-run] 3. bash $R580_DIR/open_r580_dkms_builder.sh --install $([[ "$WITH_USERSPACE" -eq 1 ]] && echo '--with-userspace')"
  echo "[dry-run] 4. update-initramfs -u && modprobe nvidia nvidia-modeset nvidia-drm nvidia-uvm"
  if [[ "$RUN_SMOKE" -eq 1 ]]; then
    echo "[dry-run] 5. Compile & run p4_cuda_nvenc_smoke.c and p4_nvdec_smoke.c"
  fi
  if [[ "$WITH_SELKIES" -eq 1 ]]; then
    echo "[dry-run] 6. nvidia-ctk cdi generate --output=/etc/cdi/nvidia.yaml && docker compose -f $R580_DIR/docker-compose.yml up -d"
  fi
  exit 0
fi

if [[ "${EUID}" -ne 0 ]]; then
  echo "ERROR: Run as root (sudo)." >&2
  exit 1
fi

# 1. Install build prerequisites
if command -v apt-get >/dev/null 2>&1; then
  DEBIAN_FRONTEND=noninteractive apt-get update -qq
  DEBIAN_FRONTEND=noninteractive apt-get install -y -qq dkms build-essential "linux-headers-$(uname -r)" curl pkg-config
elif command -v dnf >/dev/null 2>&1; then
  dnf install -y -q dkms kernel-devel kernel-headers gcc make elfutils-libelf-devel curl
fi

# 2. Download/extract 580.178.04 payload if requested
if [[ "$DOWNLOAD" -eq 1 && -z "$DRIVER_DIR" && -z "$RUN_FILE" ]]; then
  DRIVER_DIR="/tmp/r580_pkg"
  bash "$R580_DIR/extract_r580_binaries.sh" --download --output "$DRIVER_DIR"
fi

BUILDER_ARGS=()
if [[ -n "$DRIVER_DIR" ]]; then
  BUILDER_ARGS+=(--driver-dir "$DRIVER_DIR")
elif [[ -n "$RUN_FILE" ]]; then
  BUILDER_ARGS+=(--run-file "$RUN_FILE")
else
  echo "ERROR: Provide --run-file, --driver-dir, or --download." >&2
  exit 2
fi

BUILDER_ARGS+=(--install)
if [[ "$WITH_USERSPACE" -eq 1 ]]; then
  BUILDER_ARGS+=(--with-userspace)
fi

# 3. Run DKMS + userspace installer
bash "$R580_DIR/open_r580_dkms_builder.sh" "${BUILDER_ARGS[@]}"

if command -v update-initramfs >/dev/null 2>&1; then
  update-initramfs -u
elif command -v dracut >/dev/null 2>&1; then
  dracut --force
fi

modprobe nvidia nvidia-modeset nvidia-drm nvidia-uvm

# 4. Run CUDA 13.0 / NVENC 13.0 / NVDEC smoke tests
if [[ "$RUN_SMOKE" -eq 1 && "$WITH_USERSPACE" -eq 1 ]]; then
  echo "=== Running CUDA 13.0, NVENC 13.0, and NVDEC Hardware Smoke Tests ==="
  cc -O2 -Wall -Wextra -Werror "$R580_DIR/p4_cuda_nvenc_smoke.c" -ldl -o /tmp/p4_cuda_nvenc_smoke
  cc -O2 -Wall -Wextra -Werror "$R580_DIR/p4_nvdec_smoke.c" -ldl -o /tmp/p4_nvdec_smoke
  /tmp/p4_cuda_nvenc_smoke
  /tmp/p4_nvdec_smoke
  rm -f /tmp/p4_cuda_nvenc_smoke /tmp/p4_nvdec_smoke
fi

# 5. Optional Selkies + PixelFlux 10-bit 4:4:4 container startup
if [[ "$WITH_SELKIES" -eq 1 ]]; then
  if command -v nvidia-ctk >/dev/null 2>&1; then
    mkdir -p /etc/cdi /var/run/cdi
    nvidia-ctk cdi generate --output=/etc/cdi/nvidia.yaml
    cp -f /etc/cdi/nvidia.yaml /var/run/cdi/nvidia.yaml
  fi
  if command -v docker >/dev/null 2>&1; then
    docker compose -f "$R580_DIR/docker-compose.yml" up -d
  fi
fi

echo "=== open-580-driver (${ROLE}) installation complete ==="
