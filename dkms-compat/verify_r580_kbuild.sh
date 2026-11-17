#!/usr/bin/env bash
set -euo pipefail

# Clean non-destructive Kbuild compatibility smoke-checker for NVIDIA R580.178.04
# across future Ubuntu kernel header releases (Linux 7.0+). Does NOT install or modprobe.

usage() {
  cat <<'EOF'
Usage: verify_r580_kbuild.sh [--driver-dir PATH] [--kver KERNEL_RELEASE] [--work-dir PATH]

Options:
  --driver-dir PATH   Path to extracted NVIDIA-Linux-x86_64-580.178.04 directory
  --kver RELEASE      Target kernel release under /lib/modules/<RELEASE>/build (default: uname -r)
  --work-dir PATH     Temporary build tree (default: /tmp/r580_kbuild_verify)
EOF
}

DRIVER_DIR=""
KVER="$(uname -r)"
WORK_DIR="/tmp/r580_kbuild_verify"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --driver-dir) DRIVER_DIR="$2"; shift 2 ;;
    --kver)       KVER="$2"; shift 2 ;;
    --work-dir)   WORK_DIR="$2"; shift 2 ;;
    -h|--help)    usage; exit 0 ;;
    *)            echo "Unknown option: $1" >&2; usage; exit 2 ;;
  esac
done

if [[ -z "${DRIVER_DIR}" || ! -d "${DRIVER_DIR}/kernel" ]]; then
  echo "ERROR: --driver-dir must point to an extracted NVIDIA 580.178.04 tree containing kernel/" >&2
  exit 1
fi

KDIR="/lib/modules/${KVER}/build"
if [[ ! -d "${KDIR}" ]]; then
  echo "ERROR: Kernel build headers not found at ${KDIR}" >&2
  exit 1
fi

rm -rf "${WORK_DIR}"
mkdir -p "${WORK_DIR}"
cp -a "${DRIVER_DIR}/kernel/." "${WORK_DIR}/"

NPROC="$(nproc 2>/dev/null || echo 4)"
echo "=== Building R580.178.04 modules against kernel ${KVER} (-j${NPROC}) ==="
make -C "${WORK_DIR}" SYSSRC="${KDIR}" KERNEL_UNAME="${KVER}" -j"${NPROC}" modules

echo "=== Built module metadata ==="
for mod in nvidia.ko nvidia-modeset.ko nvidia-drm.ko nvidia-uvm.ko nvidia-peermem.ko; do
  if [[ -f "${WORK_DIR}/${mod}" ]]; then
    lic="$(modinfo -F license "${WORK_DIR}/${mod}")"
    ver="$(modinfo -F vermagic "${WORK_DIR}/${mod}")"
    sz="$(stat -c '%s' "${WORK_DIR}/${mod}")"
    printf "%-20s size=%-10s license=%-16s vermagic=%s\n" "${mod}" "${sz}" "${lic}" "${ver}"
  else
    echo "WARN: ${mod} not produced"
  fi
done

echo "=== Conftest #define / #undef counts ==="
for hdr in functions.h types.h symbols.h generic.h headers.h; do
  p="${WORK_DIR}/conftest/${hdr}"
  if [[ -f "${p}" ]]; then
    d="$(grep -c '^#define ' "${p}" || true)"
    u="$(grep -c '^#undef ' "${p}" || true)"
    printf "%-15s define=%-3s undef=%-3s\n" "${hdr}" "${d}" "${u}"
  fi
done

