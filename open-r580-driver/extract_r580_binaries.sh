#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# Extract and SHA-256-verify the required NVIDIA R580 (580.178.04) binary objects
# and runtime libraries needed by the Open R580 Hybrid Driver build.
set -euo pipefail

NVIDIA_VER="580.178.04"
NVIDIA_RUN_NAME="NVIDIA-Linux-x86_64-${NVIDIA_VER}.run"
NVIDIA_RUN_URL="https://download.nvidia.com/XFree86/Linux-x86_64/${NVIDIA_VER}/${NVIDIA_RUN_NAME}"

EXPECTED_NV_KERNEL_SHA256="5269a23ffba19a17fe16f076463e7ad4151254659441d1711bee6c6221ff6f4b"
EXPECTED_NV_MODESET_SHA256="9a8640a8f49c88cfcc0773d728d6b2a28b877794c95ec63a5e950b2d8385a83d"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

RUN_FILE=""
DOWNLOAD=0
FORCE=0
CACHE_DIR="${XDG_CACHE_HOME:-${HOME}/.cache}/nvidia-p4-r580"
OUTPUT_DIR="/tmp/nvidia-${NVIDIA_VER}-extracted"
BLOBS_DIR=""

usage() {
  cat <<EOF
Usage: extract_r580_binaries.sh [--run-file PATH | --download] [--output DIR] [--cache-dir DIR] [--extract-blobs DIR] [--force]

Options:
  --run-file PATH       Path to existing ${NVIDIA_RUN_NAME}
  --download            Download ${NVIDIA_RUN_NAME} from download.nvidia.com into --cache-dir (if not cached)
  --cache-dir DIR       Download cache directory (default: ${CACHE_DIR})
  --output DIR          Destination directory for extracted driver tree (default: ${OUTPUT_DIR})
  --extract-blobs DIR   Also run nv_bindata_analyzer.py to extract GP104 Falcon blobs into DIR
  --force               Force re-extraction even if --output already passes SHA-256 verification
  -h, --help            Show this help message
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --run-file)      RUN_FILE="$2"; shift 2 ;;
    --download)      DOWNLOAD=1; shift ;;
    --cache-dir)     CACHE_DIR="$2"; shift 2 ;;
    --output)        OUTPUT_DIR="$2"; shift 2 ;;
    --extract-blobs) BLOBS_DIR="$2"; shift 2 ;;
    --force)         FORCE=1; shift ;;
    -h|--help)       usage; exit 0 ;;
    *)               echo "Unknown option: $1" >&2; usage; exit 2 ;;
  esac
done

verify_extracted_tree() {
  local dir="$1"
  local nv_kobj="${dir}/kernel/nvidia/nv-kernel.o_binary"
  local nv_mobj="${dir}/kernel/nvidia-modeset/nv-modeset-kernel.o_binary"
  [[ -f "${nv_kobj}" && -f "${nv_mobj}" ]] || return 1

  local sha_k sha_m
  sha_k="$(sha256sum "${nv_kobj}" | awk '{print $1}')"
  sha_m="$(sha256sum "${nv_mobj}" | awk '{print $1}')"
  [[ "${sha_k}" == "${EXPECTED_NV_KERNEL_SHA256}" ]] || {
    echo "ERROR: nv-kernel.o_binary SHA-256 mismatch: ${sha_k}" >&2
    return 1
  }
  [[ "${sha_m}" == "${EXPECTED_NV_MODESET_SHA256}" ]] || {
    echo "ERROR: nv-modeset-kernel.o_binary SHA-256 mismatch: ${sha_m}" >&2
    return 1
  }

  local asset
  for asset in \
    "libcuda.so.${NVIDIA_VER}" \
    "libnvidia-encode.so.${NVIDIA_VER}" \
    "libnvcuvid.so.${NVIDIA_VER}" \
    "nvidia-smi"; do
    [[ -f "${dir}/${asset}" ]] || {
      echo "ERROR: Expected R580 runtime asset missing: ${dir}/${asset}" >&2
      return 1
    }
  done
  return 0
}

download_run_installer() {
  mkdir -p "${CACHE_DIR}"
  local dst="${CACHE_DIR}/${NVIDIA_RUN_NAME}"
  local tmp="${dst}.part"
  if [[ -s "${dst}" && "${FORCE}" -eq 0 ]]; then
    echo "Reusing cached installer: ${dst}" >&2
    echo "${dst}"
    return 0
  fi
  echo "Downloading ${NVIDIA_RUN_URL} -> ${dst} ..." >&2
  if command -v curl >/dev/null 2>&1; then
    curl -fL --retry 3 --retry-delay 2 -o "${tmp}" "${NVIDIA_RUN_URL}"
  elif command -v wget >/dev/null 2>&1; then
    wget -O "${tmp}" "${NVIDIA_RUN_URL}"
  else
    echo "ERROR: Neither curl nor wget is installed to download ${NVIDIA_RUN_URL}" >&2
    exit 1
  fi
  mv "${tmp}" "${dst}"
  chmod +x "${dst}"
  echo "${dst}"
}

if [[ "${FORCE}" -eq 0 ]] && verify_extracted_tree "${OUTPUT_DIR}" 2>/dev/null; then
  echo "Verified existing extracted R580 tree at ${OUTPUT_DIR}"
else
  if [[ -z "${RUN_FILE}" ]]; then
    if [[ "${DOWNLOAD}" -eq 1 ]]; then
      RUN_FILE="$(download_run_installer)"
    else
      local_default="${CACHE_DIR}/${NVIDIA_RUN_NAME}"
      if [[ -s "${local_default}" ]]; then
        RUN_FILE="${local_default}"
      else
        echo "ERROR: Specify --run-file PATH or pass --download to fetch ${NVIDIA_RUN_NAME}." >&2
        exit 1
      fi
    fi
  fi

  if [[ ! -f "${RUN_FILE}" ]]; then
    echo "ERROR: Installer not found: ${RUN_FILE}" >&2
    exit 1
  fi

  rm -rf "${OUTPUT_DIR}"
  mkdir -p "$(dirname "${OUTPUT_DIR}")"
  echo "Extracting ${RUN_FILE} -> ${OUTPUT_DIR} ..."
  sh "${RUN_FILE}" --extract-only --target "${OUTPUT_DIR}" >/dev/null

  if ! verify_extracted_tree "${OUTPUT_DIR}"; then
    echo "ERROR: Extraction completed but SHA-256 / asset verification failed." >&2
    exit 1
  fi
  echo "Verified R580.178.04 core binaries & runtime assets in ${OUTPUT_DIR}"
fi

if [[ -n "${BLOBS_DIR}" ]]; then
  if ! command -v python3 >/dev/null 2>&1; then
    echo "ERROR: python3 required for --extract-blobs" >&2
    exit 1
  fi
  mkdir -p "${BLOBS_DIR}"
  python3 "${REPO_ROOT}/tools/nv_bindata_analyzer.py" \
    "${OUTPUT_DIR}/kernel/nvidia/nv-kernel.o_binary" \
    --extract-gp104 "${BLOBS_DIR}" > "${BLOBS_DIR}/extract_summary.json"
  echo "Extracted GP104 Falcon firmware blobs into ${BLOBS_DIR}"
fi

