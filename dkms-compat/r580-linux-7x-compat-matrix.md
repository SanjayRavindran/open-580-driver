# R580.178.04 Linux 7.0+ Kernel Compatibility Matrix

## 1. Observed Baseline Build (`7.0.0-43-generic`, Ubuntu 26.04.1 LTS)

Compiling the extracted `580.178.04/kernel/` tree directly against `Linux 7.0.0-43-generic` succeeded (`exit 0`) without external patches, producing five out-of-tree kernel modules:

| Module | Observed Size (Bytes) | `MODULE_LICENSE` | Core Binary Link | Breakage Risk on Future Kernels |
| :--- | ---: | :--- | :--- | :--- |
| `nvidia.ko` | `136,222,376` | `NVIDIA` | Links MIT `nv-interface.o` + opaque `nv-kernel.o_binary` | High (shim C layer `nv.c`, `os-interface.c` vs MM/DRM/timer/objtool changes) |
| `nvidia-uvm.ko` | `60,755,816` | `Dual MIT/GPL` | Self-contained open C | High (`hmm` / `migrate_vma_*` / `mm` locking API changes) |
| `nvidia-drm.ko` | `5,574,928` | `MIT` | Self-contained open C | Very High (DRM core struct changes across `6.12+` / `7.x`) |
| `nvidia-modeset.ko` | `3,255,768` | `NVIDIA` | Links open shim + `nv-modeset-kernel.o_binary` | Medium (display interface glue; minimal hardware need on headless Tesla P4) |
| `nvidia-peermem.ko` | `402,760` | `Linux-OpenIB` | InfiniBand peer-memory glue | Low (depends on OFED / RDMA symbols) |

## 2. ABI & Licensing Separation (`nv-interface.o` vs `nv-kernel.o_binary`)

- **`nv-kernel.o_binary`** (`112,677,456` bytes, SHA-256 `5269a23ffba19a17fe16f076463e7ad4151254659441d1711bee6c6221ff6f4b`) contains **zero direct Linux kernel includes**; all OS calls route through `common/inc/nv-kernel-interface-api.h` and `common/inc/os-interface.h` implemented in the open MIT-licensed C shim (`nv-interface.o`).
- `nm nv-interface.o` reported `2,027` total symbol lines (`469` undefined external kernel symbols `U`, `611` exported/internal text symbols `T`, `71` data symbols).
- Future Linux `7.1+` / `8.x` kernel breaks therefore hit the **open-source C shim + `conftest.sh`**, not the internal Pascal RM logic inside `nv-kernel.o_binary`—meaning community DKMS compatibility patches can extend `580.178.04` for years *until* NVIDIA's x86 ABI expectations (`objtool` frame/retpoline/IBT rules or `EXPORT_SYMBOL_GPL` restrictions triggered by `license=NVIDIA`) block linking.

## 3. Observed `conftest` Fingerprint on Linux `7.0.0-43-generic`

Inspection of `kernel_build_test/conftest/` (`216` compile-test probes under `conftest/compile-tests/`):

| Header | Lines | `#define` | `#undef` | Notable Linux 6.12–7.0 Probes Reflected |
| :--- | ---: | ---: | ---: | :--- |
| `functions.h` | 85 | 50 | 34 | Defined: `NV_APERTURE_REMOVE_CONFLICTING_PCI_DEVICES_PRESENT`, `NV_DRM_CLIENT_SETUP_PRESENT`, `NV_PIN_USER_PAGES_REMOTE_PRESENT`, `NV_VMF_INSERT_MIXED_PRESENT`. Undefined: `NV_FOLLOW_PFN_PRESENT`, `NV_DRM_FBDEV_TTM_SETUP_PRESENT`. |
| `types.h` | 78 | 46 | 31 | Defined: `NV_DRM_FB_CREATE_TAKES_FORMAT_INFO`, `NV_DRM_FORMAT_INFO_HAS_IS_YUV`, `NV_DRM_CONNECTOR_HELPER_FUNCS_MODE_VALID_HAS_CONST_MODE_ARG`. Undefined: `NV_DRM_OUTPUT_POLL_CHANGED_PRESENT`, `NV_DRM_DRIVER_HAS_DATE`. |
| `symbols.h` | 40 | 36 | 3 | Defined: `NV_IS_EXPORT_SYMBOL_PRESENT_hrtimer_setup`, `NV_IS_EXPORT_SYMBOL_PRESENT_timer_delete_sync`, `NV_IS_EXPORT_SYMBOL_PRESENT_follow_pfnmap_start`, `NV_IS_EXPORT_SYMBOL_PRESENT_make_device_exclusive`, `NV_IS_EXPORT_SYMBOL_GPL___vma_start_write`. Undefined: `NV_CRYPTO_AKCIPHER_VERIFY_PRESENT`, `NV_FOLLOW_PTE_ARG1_VMA`, `NV_DMA_BUF_OPS_ATTACH_ARG2_DEV`. |
| `generic.h` | 32 | 18 | 13 | Undefined: `NV_MODULE_IMPORT_NS_TAKES_CONSTANT`, `NV_MLNX_IB_PEER_MEM_SYMBOLS_PRESENT`. |
| `headers.h` | 42 | 23 | 18 | Undefined: `NV_DRM_DRM_APERTURE_H_PRESENT`, `NV_DRM_DRM_FBDEV_GENERIC_H_PRESENT`, `NV_DRM_DRM_CLIENT_SETUP_H_PRESENT`. |

## 4. Most Likely Future Breakage Hotspots (`Linux 7.1+`)

1. **`nvidia-drm/` (`nvidia-drm-drv.c`, `nvidia-drm-fb.c`)**: Subsystem-wide DRM API refactors (`drm_mode_config_funcs`, `drm_driver`, `drm_gem_object`, `dma_buf_ops`, `dma_resv`) occur nearly every 2–3 kernel cycles.
2. **`nvidia-uvm/` & `nvidia/os-interface.c` MM helpers**: `follow_pfnmap_start` / `migrate_vma_setup` / `make_device_exclusive` / `vma_start_write` are `EXPORT_SYMBOL_GPL` or fast-evolving MM primitives; any signature shift requires updating `conftest/compile-tests/` + shim callers.
3. **x86-64 `objtool` / `retbleed` / `FINEIBT` enforcement**: Because `nv-kernel.o_binary` uses `--use_altstack` / non-standard frame conventions, future kernel `objtool` strictness changes can reject link validation of `nvidia.ko` even when all C shims compile cleanly.

