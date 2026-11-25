# Disclaimer, Responsibility Notice & AI/LLM Disclosure

## 1. No Warranty & No Responsibility Notice

THIS SOFTWARE, KERNEL SHIM CONFIGURATION, RUST CODEBASE, REVERSE-ENGINEERING TOOLING, AND TECHNICAL DOCUMENTATION ARE PROVIDED **"AS IS"**, WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE, NON-INFRINGEMENT, OR SYSTEM STABILITY.

BY USING, COMPILING, MODIFYING, OR INSTALLING ANYTHING IN THIS REPOSITORY (INCLUDING KERNEL MODULES, `modprobe`/`udev` RULES, USERSPACE SHIMS, OR CONTAINER CONFIGURATIONS), YOU ACCEPT FULL AND SOLE RESPONSIBILITY FOR ALL OUTCOMES. THE AUTHORS, MAINTAINERS, AND CONTRIBUTORS ACCEPT **NO RESPONSIBILITY OR LIABILITY WHATSOEVER** FOR:

- Kernel panics, system lockups, boot failures, or initramfs corruption,
- Hardware malfunction, GPU instability, thermal issues, or physical damage,
- Data loss, filesystem corruption, or service outages,
- Any compliance obligations or third-party licensing terms triggered when you extract and install NVIDIA's proprietary driver components on your machine.

Kernel-level software and out-of-tree DKMS packages carry inherent operational risk. **Always back up critical data, maintain console/recovery access, and run `--dry-run` and tests before installing onto any host.**

---

## 2. Large Language Model (LLM) Codebase Disclosure

An **AI / Large Language Model (LLM)** was used during the reverse-engineering synthesis and to generate the open-source codebase in this repository — including:

- Python ELF `.rodata` / Falcon descriptor / VBIOS / PROM parsers and unit tests ([`tools/`](./tools/), [`tests/`](./tests/)),
- GPLv2 clean-room telemetry kernel module ([`kmods/p4_cleanroom_diag/`](./kmods/p4_cleanroom_diag/)),
- Track 1 extraction, DKMS staging, ABI verification, and C smoke-test tooling ([`open-r580-driver/`](./open-r580-driver/)),
- GitHub Actions CI/CD pipelines and the technical documentation suite ([`docs/`](./docs/)).

Users and maintainers should **independently audit and review** all generated code, shell scripts, and kernel configuration files prior to deployment.

---

## 3. Clean-Room Open-Source Boundary & Licensing

- **Original Open-Source Code (`GPL-2.0-only`):** All original source code, build scripts, tests, and documentation in this repository were written without incorporating other third-party open-source projects and are licensed under the **GNU General Public License v2.0** ([`LICENSE`](./LICENSE)).
- **No NVIDIA Proprietary Binaries Redistributed:** This repository does **NOT** redistribute NVIDIA `.run` installers, `nv-kernel.o_binary`, `nv-modeset-kernel.o_binary`, Falcon signed microcode (`.bin`), VBIOS/PROM dumps (`.rom`), or proprietary userspace libraries (`.so`).
- **End-User Extraction:** Required vendor objects and signed firmware blobs needed for the Track 1 hybrid build or Track 2 Falcon bootstrap must be extracted locally by the user from NVIDIA's publicly distributed Linux driver package (`580.178.04`) via [`open-r580-driver/extract_r580_binaries.sh`](./open-r580-driver/extract_r580_binaries.sh) under NVIDIA's applicable software license terms.

