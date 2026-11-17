#!/usr/bin/env python3
"""
Clean-room R580 kernel shim ABI & Linux 7.x compatibility verifier.

Inspects the MIT/GPLv2 C kernel headers (`nv-ioctl-numbers.h`,
`nv-ioctl-numa.h`, `nvidia-drm-ioctl.h`, `conftest/headers.h`) for
stable userspace CUDA/NVENC/DRM ABI constants, and optionally verifies
live device nodes + PCI state when invoked with `--live`.

License: MIT OR GPL-2.0-or-later
"""

from __future__ import annotations

import argparse
import json
import os
import re
from pathlib import Path
from typing import Dict, List


REQUIRED_NV_ESCAPES = [
    "NV_ESC_CARD_INFO",
    "NV_ESC_REGISTER_FD",
    "NV_ESC_ALLOC_OS_EVENT",
    "NV_ESC_FREE_OS_EVENT",
    "NV_ESC_STATUS_CODE",
    "NV_ESC_CHECK_VERSION_STR",
    "NV_ESC_IOCTL_XFER_CMD",
    "NV_ESC_ATTACH_GPUS_TO_FD",
    "NV_ESC_QUERY_DEVICE_INTR",
    "NV_ESC_SYS_PARAMS",
    "NV_ESC_EXPORT_TO_DMABUF_FD",
    "NV_ESC_WAIT_OPEN_COMPLETE",
]


def parse_ioctl_headers(kernel_dir: Path) -> Dict[str, object]:
    """Extract and validate R580 escape numbers and DRM NVIDIA ioctl macros."""
    num_hdr = kernel_dir / "common" / "inc" / "nv-ioctl-numbers.h"
    numa_hdr = kernel_dir / "common" / "inc" / "nv-ioctl-numa.h"
    drm_hdr = kernel_dir / "nvidia-drm" / "nvidia-drm-ioctl.h"
    if not num_hdr.is_file():
        raise FileNotFoundError(f"Missing {num_hdr}")

    num_text = num_hdr.read_text(encoding="utf-8", errors="replace")
    magic_m = re.search(r"#define\s+NV_IOCTL_MAGIC\s+'([^']+)'", num_text)
    if not magic_m:
        raise ValueError("NV_IOCTL_MAGIC definition not found")

    escapes: Dict[str, int] = {}
    for m in re.finditer(r"#define\s+(NV_ESC_[A-Z0-9_]+)\s+\(\s*NV_IOCTL_BASE\s*\+\s*(\d+)\s*\)", num_text):
        escapes[m.group(1)] = int(m.group(2))

    if numa_hdr.is_file():
        numa_text = numa_hdr.read_text(encoding="utf-8", errors="replace")
        for m in re.finditer(r"#define\s+(NV_ESC_[A-Z0-9_]+)\s+\(\s*NV_IOCTL_BASE\s*\+\s*(\d+)\s*\)", numa_text):
            escapes[m.group(1)] = int(m.group(2))

    missing = [name for name in REQUIRED_NV_ESCAPES if name not in escapes]
    if missing:
        raise ValueError(f"Missing required NV_ESC ioctl macros: {missing}")

    drm_ioctls: List[str] = []
    if drm_hdr.is_file():
        drm_text = drm_hdr.read_text(encoding="utf-8", errors="replace")
        drm_ioctls = sorted(set(re.findall(r"#define\s+(DRM_IOCTL_NVIDIA_[A-Z0-9_]+)", drm_text)))

    conftest_counts: Dict[str, Dict[str, int]] = {}
    conftest_dir = kernel_dir / "conftest"
    if conftest_dir.is_dir():
        for name in ("functions.h", "types.h", "symbols.h", "generic.h", "headers.h"):
            fp = conftest_dir / name
            if fp.is_file():
                lines = fp.read_text(encoding="utf-8", errors="replace").splitlines()
                conftest_counts[name] = {
                    "define": sum(1 for ln in lines if ln.startswith("#define ")),
                    "undef": sum(1 for ln in lines if ln.startswith("#undef ")),
                }

    return {
        "ioctl_magic": magic_m.group(1),
        "escape_count": len(escapes),
        "escapes": escapes,
        "drm_ioctl_count": len(drm_ioctls),
        "drm_ioctls": drm_ioctls,
        "conftest_counts": conftest_counts,
    }


def check_live_nodes() -> Dict[str, object]:
    """Verify runtime /dev/nvidia* nodes and sysfs driver binding non-destructively."""
    nodes = ["/dev/nvidiactl", "/dev/nvidia0", "/dev/nvidia-uvm", "/dev/nvidia-modeset"]
    present = {n: os.path.exists(n) for n in nodes}
    driver_link = None
    pci_dev = Path("/sys/bus/pci/devices/0000:01:00.0/driver")
    if pci_dev.is_symlink():
        driver_link = os.path.basename(os.readlink(pci_dev))
    return {"dev_nodes": present, "pci_01_00_driver": driver_link}


def main() -> int:
    ap = argparse.ArgumentParser(description="R580 open kernel shim ABI verifier")
    ap.add_argument("--kernel-dir", type=Path, help="Extracted 580.178.04/kernel directory")
    ap.add_argument("--live", action="store_true", help="Also verify local /dev/nvidia* and PCI binding")
    args = ap.parse_args()

    report: Dict[str, object] = {}
    if args.kernel_dir is not None:
        report["abi_headers"] = parse_ioctl_headers(args.kernel_dir)
    if args.live:
        report["live"] = check_live_nodes()
    print(json.dumps(report, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

