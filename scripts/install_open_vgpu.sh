#!/usr/bin/env bash
set -euo pipefail

# Unified Host & Guest Installer for open-vgpu-tesla-p4 (535.309.01-vgpu-kvm + Open Host Daemons & Hybrid Guests)
# Delegates to open-pascal-vgpu/scripts/install_open_vgpu.sh (or OPEN_VGPU_REPO).

OPEN_VGPU_REPO="${OPEN_VGPU_REPO:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../../open-pascal-vgpu" 2>/dev/null && pwd || true)}"
DRY_RUN=0
PASS_ARGS=()

for arg in "$@"; do
  if [[ "$arg" == "--dry-run" ]]; then
    DRY_RUN=1
  fi
  PASS_ARGS+=("$arg")
done

if [[ -n "$OPEN_VGPU_REPO" && -f "$OPEN_VGPU_REPO/scripts/install_open_vgpu.sh" ]]; then
  exec bash "$OPEN_VGPU_REPO/scripts/install_open_vgpu.sh" "${PASS_ARGS[@]}"
fi

if [[ "$DRY_RUN" -eq 1 ]]; then
  echo "[dry-run] Delegate to open-pascal-vgpu/scripts/install_open_vgpu.sh ${PASS_ARGS[*]}"
  exit 0
fi

echo "ERROR: Companion open-pascal-vgpu repository not found at '${OPEN_VGPU_REPO}'. Set OPEN_VGPU_REPO=/path/to/open-pascal-vgpu." >&2
exit 1
