#!/usr/bin/env bash
set -euo pipefail

# Clean Track 1 DKMS & open-shim builder for NVIDIA Tesla P4 (GP104GL)
# using R580.178.04 MIT/GPLv2 kernel shims + SHA-256 verified binary cores.

EXPECTED_NV_KERNEL_SHA256="5269a23ffba19a17fe16f076463e7ad4151254659441d1711bee6c6221ff6f4b"
EXPECTED_NV_MODESET_SHA256="9a8640a8f49c88cfcc0773d728d6b2a28b877794c95ec63a5e950b2d8385a83d"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DRIVER_DIR=""
RUN_FILE=""
NVIDIA_VER="580.178.04"
STAGE_DIR="/tmp/nvidia-gp104-r580-${NVIDIA_VER}"
KVER="$(uname -r)"
MODE="dry-run"
WITH_USERSPACE=0
LIBDIR="/usr/lib/x86_64-linux-gnu"
STATE_DIR="/var/lib/nvidia-gp104-r580"
MANIFEST_FILE="${STATE_DIR}/installed.manifest"

usage() {
  cat <<'EOF'
Usage: open_r580_dkms_builder.sh [--driver-dir PATH | --run-file PATH] [--kver RELEASE] [--stage-dir PATH] [--dry-run | --install [--with-userspace] | --uninstall]

Options:
  --driver-dir PATH   Extracted NVIDIA-Linux-x86_64-580.178.04 directory containing kernel/
  --run-file PATH     Path to NVIDIA-Linux-x86_64-580.178.04.run (extracted into temporary folder)
  --kver RELEASE      Target kernel release under /lib/modules/<RELEASE>/build (default: uname -r)
  --stage-dir PATH    Staging directory for --dry-run build (default: /tmp/nvidia-gp104-r580-580.178.04)
  --dry-run           Verify SHA-256, stage dkms.conf, and Kbuild-compile without root install (default)
  --install           Requires root: install DKMS tree, nouveau blacklist, and udev rule
  --with-userspace    Use with --install: install non-clobbering R580 CUDA/NVENC/NVDEC/GLVND/Vulkan/OpenCL runtime
  --uninstall         Requires root: unload active nvidia modules, remove DKMS/config/tracked userspace files
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --driver-dir)     DRIVER_DIR="$2"; shift 2 ;;
    --run-file)       RUN_FILE="$2"; shift 2 ;;
    --kver)           KVER="$2"; shift 2 ;;
    --stage-dir)      STAGE_DIR="$2"; shift 2 ;;
    --dry-run)        MODE="dry-run"; shift ;;
    --install)        MODE="install"; shift ;;
    --with-userspace) WITH_USERSPACE=1; shift ;;
    --uninstall)      MODE="uninstall"; shift ;;
    -h|--help)        usage; exit 0 ;;
    *)                echo "Unknown option: $1" >&2; usage; exit 2 ;;
  esac
done

unload_running_nvidia() {
  for mod in nvidia_drm nvidia_uvm nvidia_modeset nvidia; do
    if lsmod | awk '{print $1}' | grep -qx "${mod}"; then
      modprobe -r "${mod}" 2>/dev/null || rmmod "${mod}" || {
        echo "ERROR: Could not unload ${mod}; stop processes using /dev/nvidia* first." >&2
        return 1
      }
    fi
  done
}

do_uninstall() {
  if [[ "${EUID}" -ne 0 ]]; then
    echo "ERROR: --uninstall must run as root" >&2
    exit 1
  fi
  unload_running_nvidia
  if command -v dkms >/dev/null 2>&1; then
    dkms remove -m nvidia-gp104-r580 -v "${NVIDIA_VER}" --all >/dev/null 2>&1 || true
  fi
  rm -rf "/usr/src/nvidia-gp104-r580-${NVIDIA_VER}"
  rm -f /etc/modprobe.d/blacklist-nouveau.conf /etc/modules-load.d/nvidia-p4.conf /etc/udev/rules.d/70-nvidia-p4.rules
  find "/lib/modules/${KVER}" \( -name 'nvidia*.ko' -o -name 'nvidia*.ko.*' \) -delete 2>/dev/null || true
  if [[ -f "${MANIFEST_FILE}" ]]; then
    tac "${MANIFEST_FILE}" | while IFS= read -r target; do
      [[ -n "${target}" ]] && rm -f "${target}"
    done
    rm -f "${MANIFEST_FILE}"
  fi
  rmdir "${STATE_DIR}" 2>/dev/null || true
  depmod -a "${KVER}" || true
  ldconfig || true
  echo "Uninstalled NVIDIA GP104 R580 kernel modules, config, and tracked userspace runtime."
}

if [[ "${MODE}" == "uninstall" ]]; then
  do_uninstall
  exit 0
fi

TMP_EXTRACT=""
cleanup() {
  if [[ -n "${TMP_EXTRACT}" && -d "${TMP_EXTRACT}" ]]; then
    rm -rf "${TMP_EXTRACT}"
  fi
}
trap cleanup EXIT

if [[ -n "${RUN_FILE}" ]]; then
  if [[ ! -f "${RUN_FILE}" ]]; then
    echo "ERROR: --run-file not found: ${RUN_FILE}" >&2
    exit 1
  fi
  TMP_EXTRACT="$(mktemp -d /tmp/r580_extract.XXXXXX)"
  bash "${RUN_FILE}" --extract-only --target "${TMP_EXTRACT}/pkg" >/dev/null
  DRIVER_DIR="${TMP_EXTRACT}/pkg"
fi

if [[ -z "${DRIVER_DIR}" || ! -d "${DRIVER_DIR}/kernel" ]]; then
  echo "ERROR: Supply --driver-dir or --run-file pointing to NVIDIA 580.178.04" >&2
  exit 1
fi

KDIR="/lib/modules/${KVER}/build"
if [[ ! -d "${KDIR}" ]]; then
  echo "ERROR: Kernel build directory missing: ${KDIR}" >&2
  exit 1
fi

NV_KOBJ="${DRIVER_DIR}/kernel/nvidia/nv-kernel.o_binary"
NV_MOBJ="${DRIVER_DIR}/kernel/nvidia-modeset/nv-modeset-kernel.o_binary"
for f in "${NV_KOBJ}" "${NV_MOBJ}"; do
  if [[ ! -f "${f}" ]]; then
    echo "ERROR: Missing core link artifact: ${f}" >&2
    exit 1
  fi
done

actual_k_sha="$(sha256sum "${NV_KOBJ}" | awk '{print $1}')"
actual_m_sha="$(sha256sum "${NV_MOBJ}" | awk '{print $1}')"
if [[ "${actual_k_sha}" != "${EXPECTED_NV_KERNEL_SHA256}" ]]; then
  echo "ERROR: nv-kernel.o_binary SHA-256 mismatch: ${actual_k_sha}" >&2
  exit 1
fi
if [[ "${actual_m_sha}" != "${EXPECTED_NV_MODESET_SHA256}" ]]; then
  echo "ERROR: nv-modeset-kernel.o_binary SHA-256 mismatch: ${actual_m_sha}" >&2
  exit 1
fi
echo "Verified R580.178.04 core SHA-256 checksums."

NPROC="$(nproc 2>/dev/null || echo 4)"
TARGET_SRC="${STAGE_DIR}"
if [[ "${MODE}" == "install" ]]; then
  if [[ "${EUID}" -ne 0 ]]; then
    echo "ERROR: --install must run as root" >&2
    exit 1
  fi
  TARGET_SRC="/usr/src/nvidia-gp104-r580-580.178.04"
fi

rm -rf "${TARGET_SRC}"
mkdir -p "${TARGET_SRC}"
cp -a "${DRIVER_DIR}/kernel/." "${TARGET_SRC}/"
sed "s/__JOBS/${NPROC}/g" "${SCRIPT_DIR}/dkms.conf.template" > "${TARGET_SRC}/dkms.conf"
echo "Staged open kernel shim + verified blobs in ${TARGET_SRC}"

if [[ "${MODE}" == "dry-run" ]]; then
  make -C "${TARGET_SRC}" NV_EXCLUDE_BUILD_MODULES="nvidia-peermem" \
       SYSSRC="${KDIR}" KERNEL_UNAME="${KVER}" -j"${NPROC}" modules
  for m in nvidia.ko nvidia-modeset.ko nvidia-drm.ko nvidia-uvm.ko; do
    if [[ ! -f "${TARGET_SRC}/${m}" ]]; then
      echo "ERROR: Expected module ${m} was not built" >&2
      exit 1
    fi
    printf "Built %-20s license=%s\n" "${m}" "$(modinfo -F license "${TARGET_SRC}/${m}")"
  done
  exit 0
fi

record_path() {
  echo "$1" >> "${MANIFEST_FILE}"
}

install_tracked() {
  local mode="$1" src="$2" dst="$3"
  if [[ ! -e "${src}" ]]; then
    echo "WARN: Missing optional asset ${src}; skipping ${dst}" >&2
    return 0
  fi
  install -D -m "${mode}" "${src}" "${dst}"
  record_path "${dst}"
}

install_symlink_tracked() {
  local target="$1" linkpath="$2"
  mkdir -p "$(dirname "${linkpath}")"
  ln -sfn "${target}" "${linkpath}"
  record_path "${linkpath}"
}

install_lib_bundle() {
  local src_name="$1" soname="${2:-}" devname="${3:-}"
  local src="${DRIVER_DIR}/${src_name}"
  if [[ ! -f "${src}" ]]; then
    echo "WARN: Missing library ${src}; skipping" >&2
    return 0
  fi
  install_tracked 0755 "${src}" "${LIBDIR}/${src_name}"
  if [[ -n "${soname}" && "${soname}" != "${src_name}" ]]; then
    install_symlink_tracked "${src_name}" "${LIBDIR}/${soname}"
  fi
  if [[ -n "${devname}" ]]; then
    install_symlink_tracked "${soname:-${src_name}}" "${LIBDIR}/${devname}"
  fi
}

install_fallback_dispatcher() {
  local src_name="$1" soname="$2" devname="$3"
  # Preserve distro GLVND / OpenCL neutral wrappers when owned by apt (libglvnd0/libegl1/ocl-icd-libopencl1).
  if [[ -e "${LIBDIR}/${soname}" ]]; then
    return 0
  fi
  install_lib_bundle "${src_name}" "${soname}" "${devname}"
}

install_userspace() {
  mkdir -p "${STATE_DIR}"
  : > "${MANIFEST_FILE}"

  # Userspace tools (nvidia-modprobe keeps setuid bit per .manifest for unprivileged container/device setup).
  install_tracked 4755 "${DRIVER_DIR}/nvidia-modprobe" /usr/bin/nvidia-modprobe
  local bin
  for bin in nvidia-smi nvidia-persistenced nvidia-debugdump nvidia-cuda-mps-control nvidia-cuda-mps-server nvidia-pcc nvidia-ngx-updater nvidia-bug-report.sh; do
    install_tracked 0755 "${DRIVER_DIR}/${bin}" "/usr/bin/${bin}"
  done

  # Core CUDA / NVENC / NVDEC / NVML / OpenGL / EGL / Vulkan / FBC vendor libraries.
  local spec src_name soname devname
  local -a lib_specs=(
    "libnvidia-ml.so.${NVIDIA_VER}:libnvidia-ml.so.1:libnvidia-ml.so"
    "libcuda.so.${NVIDIA_VER}:libcuda.so.1:libcuda.so"
    "libnvidia-opencl.so.${NVIDIA_VER}:libnvidia-opencl.so.1:"
    "libnvidia-ptxjitcompiler.so.${NVIDIA_VER}:libnvidia-ptxjitcompiler.so.1:libnvidia-ptxjitcompiler.so"
    "libcudadebugger.so.${NVIDIA_VER}:libcudadebugger.so.1:"
    "libnvidia-nvvm.so.${NVIDIA_VER}:libnvidia-nvvm.so.4:libnvidia-nvvm.so"
    "libnvidia-nvvm70.so.4::"
    "libnvidia-gpucomp.so.${NVIDIA_VER}::"
    "libnvidia-api.so.1::"
    "libnvidia-glcore.so.${NVIDIA_VER}::"
    "libnvidia-tls.so.${NVIDIA_VER}::"
    "libnvidia-glsi.so.${NVIDIA_VER}::"
    "libnvidia-glvkspirv.so.${NVIDIA_VER}::"
    "libGLX_nvidia.so.${NVIDIA_VER}:libGLX_nvidia.so.0:"
    "libnvidia-eglcore.so.${NVIDIA_VER}::"
    "libEGL_nvidia.so.${NVIDIA_VER}:libEGL_nvidia.so.0:"
    "libGLESv2_nvidia.so.${NVIDIA_VER}:libGLESv2_nvidia.so.2:"
    "libGLESv1_CM_nvidia.so.${NVIDIA_VER}:libGLESv1_CM_nvidia.so.1:"
    "libnvidia-egl-wayland.so.1.1.20:libnvidia-egl-wayland.so.1:"
    "libnvidia-egl-gbm.so.1.1.3:libnvidia-egl-gbm.so.1:"
    "libnvidia-egl-xlib.so.1.0.5:libnvidia-egl-xlib.so.1:"
    "libnvidia-egl-xcb.so.1.0.5:libnvidia-egl-xcb.so.1:"
    "libnvidia-cfg.so.${NVIDIA_VER}:libnvidia-cfg.so.1:libnvidia-cfg.so"
    "libnvidia-allocator.so.${NVIDIA_VER}:libnvidia-allocator.so.1:libnvidia-allocator.so"
    "libnvidia-rtcore.so.${NVIDIA_VER}::"
    "libnvoptix.so.${NVIDIA_VER}:libnvoptix.so.1:"
    "libnvidia-ngx.so.${NVIDIA_VER}::"
    "libnvidia-fbc.so.${NVIDIA_VER}:libnvidia-fbc.so.1:libnvidia-fbc.so"
    "libnvcuvid.so.${NVIDIA_VER}:libnvcuvid.so.1:libnvcuvid.so"
    "libnvidia-encode.so.${NVIDIA_VER}:libnvidia-encode.so.1:libnvidia-encode.so"
    "libnvidia-opticalflow.so.${NVIDIA_VER}:libnvidia-opticalflow.so.1:libnvidia-opticalflow.so"
    "libnvidia-pkcs11.so.${NVIDIA_VER}::"
    "libnvidia-pkcs11-openssl3.so.${NVIDIA_VER}::"
    "libnvidia-present.so.${NVIDIA_VER}::"
    "libnvidia-vksc-core.so.${NVIDIA_VER}:libnvidia-vksc-core.so.1:"
    "libnvidia-sandboxutils.so.${NVIDIA_VER}:libnvidia-sandboxutils.so.1:libnvidia-sandboxutils.so"
    "libnvidia-wayland-client.so.${NVIDIA_VER}::"
  )
  for spec in "${lib_specs[@]}"; do
    IFS=':' read -r src_name soname devname <<< "${spec}"
    install_lib_bundle "${src_name}" "${soname}" "${devname}"
  done

  # Neutral dispatchers installed ONLY if missing on minimal hosts without Mesa/GLVND/OpenCL packages.
  install_fallback_dispatcher "libEGL.so.1.1.0" "libEGL.so.1" "libEGL.so"
  install_fallback_dispatcher "libOpenGL.so.0" "libOpenGL.so.0" "libOpenGL.so"
  install_fallback_dispatcher "libGLESv2.so.2.1.0" "libGLESv2.so.2" "libGLESv2.so"
  install_fallback_dispatcher "libGLESv1_CM.so.1.2.0" "libGLESv1_CM.so.1" "libGLESv1_CM.so"
  install_fallback_dispatcher "libOpenCL.so.1.0.0" "libOpenCL.so.1" "libOpenCL.so"

  # VDPAU, GBM & Xorg GLX modules (isolated under xorg/modules so Intel i915 KMS stays undisturbed).
  install_tracked 0755 "${DRIVER_DIR}/libvdpau_nvidia.so.${NVIDIA_VER}" "${LIBDIR}/vdpau/libvdpau_nvidia.so.${NVIDIA_VER}"
  install_symlink_tracked "libvdpau_nvidia.so.${NVIDIA_VER}" "${LIBDIR}/vdpau/libvdpau_nvidia.so.1"
  install_symlink_tracked "vdpau/libvdpau_nvidia.so.${NVIDIA_VER}" "${LIBDIR}/libvdpau_nvidia.so"
  install_symlink_tracked "../libnvidia-allocator.so.1" "${LIBDIR}/gbm/nvidia-drm_gbm.so"
  install_tracked 0755 "${DRIVER_DIR}/nvidia_drv.so" /usr/lib/xorg/modules/drivers/nvidia_drv.so
  install_tracked 0755 "${DRIVER_DIR}/libglxserver_nvidia.so.${NVIDIA_VER}" "/usr/lib/xorg/modules/extensions/libglxserver_nvidia.so.${NVIDIA_VER}"
  install_symlink_tracked "libglxserver_nvidia.so.${NVIDIA_VER}" /usr/lib/xorg/modules/extensions/libglxserver_nvidia.so

  # ICD / EGL external platform / Vulkan / OpenCL / OptiX / Sandboxutils metadata.
  install_tracked 0644 "${DRIVER_DIR}/10_nvidia.json" /usr/share/glvnd/egl_vendor.d/10_nvidia.json
  install_tracked 0644 "${DRIVER_DIR}/nvidia_icd.json" /usr/share/vulkan/icd.d/nvidia_icd.json
  install_tracked 0644 "${DRIVER_DIR}/nvidia_layers.json" /usr/share/vulkan/implicit_layer.d/nvidia_layers.json
  install_tracked 0644 "${DRIVER_DIR}/nvidia_icd_vksc.json" /usr/share/vulkansc/icd.d/nvidia_icd_vksc.json
  install_tracked 0444 "${DRIVER_DIR}/nvidia.icd" /etc/OpenCL/vendors/nvidia.icd
  local ext_json
  for ext_json in 10_nvidia_wayland.json 15_nvidia_gbm.json 20_nvidia_xlib.json 20_nvidia_xcb.json; do
    install_tracked 0644 "${DRIVER_DIR}/${ext_json}" "/usr/share/egl/egl_external_platform.d/${ext_json}"
  done
  install_tracked 0444 "${DRIVER_DIR}/nvoptix.bin" /usr/share/nvidia/nvoptix.bin
  install_tracked 0444 "${DRIVER_DIR}/nvidia-application-profiles-${NVIDIA_VER}-rc" "/usr/share/nvidia/nvidia-application-profiles-${NVIDIA_VER}-rc"
  install_tracked 0444 "${DRIVER_DIR}/nvidia-application-profiles-${NVIDIA_VER}-key-documentation" "/usr/share/nvidia/nvidia-application-profiles-${NVIDIA_VER}-key-documentation"
  install_tracked 0644 "${DRIVER_DIR}/sandboxutils-filelist.json" /usr/share/nvidia/sandboxutils-filelist.json

  ldconfig
  echo "Installed non-clobbering R580 userspace runtime ($(wc -l < "${MANIFEST_FILE}") paths tracked)."
}

unload_running_nvidia
install -m 0644 "${SCRIPT_DIR}/modprobe-blacklist-nouveau.conf" /etc/modprobe.d/blacklist-nouveau.conf
install -m 0644 "${SCRIPT_DIR}/70-nvidia-p4.rules" /etc/udev/rules.d/70-nvidia-p4.rules
cat <<'EOF' > /etc/modules-load.d/nvidia-p4.conf
nvidia
nvidia-modeset
nvidia-drm
nvidia-uvm
EOF

if command -v dkms >/dev/null 2>&1; then
  dkms remove -m nvidia-gp104-r580 -v "${NVIDIA_VER}" --all >/dev/null 2>&1 || true
  dkms add -m nvidia-gp104-r580 -v "${NVIDIA_VER}"
  dkms build -m nvidia-gp104-r580 -v "${NVIDIA_VER}" -k "${KVER}"
  dkms install -m nvidia-gp104-r580 -v "${NVIDIA_VER}" -k "${KVER}"
else
  echo "WARN: dkms binary not installed; falling back to Kbuild modules_install" >&2
  make -C "${TARGET_SRC}" NV_EXCLUDE_BUILD_MODULES="nvidia-peermem" \
       SYSSRC="${KDIR}" KERNEL_UNAME="${KVER}" -j"${NPROC}" modules_install
  depmod -a "${KVER}"
fi

if [[ "${WITH_USERSPACE}" -eq 1 ]]; then
  install_userspace
fi

if command -v udevadm >/dev/null 2>&1; then
  udevadm control --reload-rules >/dev/null 2>&1 || true
fi

echo "Install complete. Load with 'modprobe nvidia nvidia-modeset nvidia-drm nvidia-uvm' and run 'update-initramfs -u' to persist nouveau blacklist at boot."

