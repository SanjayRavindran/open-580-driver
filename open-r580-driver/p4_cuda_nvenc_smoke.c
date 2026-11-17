// SPDX-License-Identifier: MIT
/*
 * p4_cuda_nvenc_smoke.c - Runtime smoke test verifying CUDA compute + VRAM DMA
 * and NVENC hardware encoder availability on Tesla P4 via R580 userspace libs.
 */

#include <dlfcn.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

typedef int CUresult;
typedef int CUdevice;
typedef void *CUcontext;
typedef uint64_t CUdeviceptr;

typedef struct {
    uint32_t version;
    uint32_t deviceType;
    void *device;
    uint32_t *reserved;
    uint32_t apiVersion;
    uint32_t reserved1[253];
    void *reserved2[64];
} nv_enc_open_ex_params_t;

int main(void)
{
    void *h_cuda = dlopen("libcuda.so.1", RTLD_NOW | RTLD_GLOBAL);
    void *h_enc;
    CUresult (*p_cuInit)(unsigned int);
    CUresult (*p_cuDeviceGetCount)(int *);
    CUresult (*p_cuDeviceGet)(CUdevice *, int);
    CUresult (*p_cuDeviceGetName)(char *, int, CUdevice);
    CUresult (*p_cuDeviceTotalMem)(size_t *, CUdevice);
    CUresult (*p_cuCtxCreate)(CUcontext *, unsigned int, CUdevice);
    CUresult (*p_cuMemAlloc)(CUdeviceptr *, size_t);
    CUresult (*p_cuMemsetD8)(CUdeviceptr, unsigned char, size_t);
    CUresult (*p_cuMemcpyDtoH)(void *, CUdeviceptr, size_t);
    CUresult (*p_cuMemFree)(CUdeviceptr);
    CUresult (*p_cuCtxDestroy)(CUcontext);
    int count = 0;
    CUdevice dev = 0;
    CUcontext ctx = NULL;
    CUdeviceptr dptr = 0;
    char dev_name[128] = {0};
    size_t vram_bytes = 0;
    uint8_t readback[8] = {0};
    int rc;

    if (!h_cuda) {
        fprintf(stderr, "dlopen(libcuda.so.1) failed: %s\n", dlerror());
        return 1;
    }

    p_cuInit = dlsym(h_cuda, "cuInit");
    p_cuDeviceGetCount = dlsym(h_cuda, "cuDeviceGetCount");
    p_cuDeviceGet = dlsym(h_cuda, "cuDeviceGet");
    p_cuDeviceGetName = dlsym(h_cuda, "cuDeviceGetName");
    p_cuDeviceTotalMem = dlsym(h_cuda, "cuDeviceTotalMem_v2");
    p_cuCtxCreate = dlsym(h_cuda, "cuCtxCreate_v2");
    p_cuMemAlloc = dlsym(h_cuda, "cuMemAlloc_v2");
    p_cuMemsetD8 = dlsym(h_cuda, "cuMemsetD8_v2");
    p_cuMemcpyDtoH = dlsym(h_cuda, "cuMemcpyDtoH_v2");
    p_cuMemFree = dlsym(h_cuda, "cuMemFree_v2");
    p_cuCtxDestroy = dlsym(h_cuda, "cuCtxDestroy_v2");

    if (!p_cuInit || !p_cuDeviceGetCount || !p_cuCtxCreate || !p_cuMemAlloc) {
        fprintf(stderr, "Missing required CUDA driver symbols\n");
        dlclose(h_cuda);
        return 2;
    }

    rc = p_cuInit(0);
    if (rc != 0 || p_cuDeviceGetCount(&count) != 0 || count <= 0) {
        fprintf(stderr, "CUDA init/device check failed: rc=%d count=%d\n", rc, count);
        dlclose(h_cuda);
        return 3;
    }

    p_cuDeviceGet(&dev, 0);
    p_cuDeviceGetName(dev_name, sizeof(dev_name), dev);
    p_cuDeviceTotalMem(&vram_bytes, dev);
    printf("CUDA[0]: %s (%zu MiB)\n", dev_name, vram_bytes / (1024U * 1024U));

    if (p_cuCtxCreate(&ctx, 0, dev) != 0 || p_cuMemAlloc(&dptr, 4096) != 0) {
        fprintf(stderr, "CUDA context/VRAM allocation failed\n");
        dlclose(h_cuda);
        return 4;
    }

    p_cuMemsetD8(dptr, 0x5A, 4096);
    p_cuMemcpyDtoH(readback, dptr, sizeof(readback));
    printf("VRAM DMA check: %02x %02x %02x %02x\n",
           readback[0], readback[1], readback[2], readback[3]);

    h_enc = dlopen("libnvidia-encode.so.1", RTLD_NOW);
    if (h_enc) {
        int (*p_getMaxVer)(uint32_t *) = dlsym(h_enc, "NvEncodeAPIGetMaxSupportedVersion");
        int (*p_createInst)(void *) = dlsym(h_enc, "NvEncodeAPICreateInstance");
        uint32_t max_ver = 0;
        if (p_getMaxVer && p_createInst && p_getMaxVer(&max_ver) == 0) {
            uint32_t maj = max_ver >> 4;
            uint32_t min = max_ver & 0xfU;
            uint32_t api_ver = maj | (min << 24);
            void *ptrs[256];

            memset(ptrs, 0, sizeof(ptrs));
            {
                uint32_t hdr_ver = api_ver | (2U << 16) | (0x7U << 28);
                memcpy(&ptrs[0], &hdr_ver, sizeof(hdr_ver));
            }
            if (p_createInst(ptrs) == 0 && ptrs[30] && ptrs[2]) {
                int (*open_ex)(void *, void **) = ptrs[30];
                int (*get_guid_cnt)(void *, uint32_t *) = ptrs[2];
                nv_enc_open_ex_params_t params;
                void *enc = NULL;
                uint32_t guid_cnt = 0;

                memset(&params, 0, sizeof(params));
                params.version = api_ver | (1U << 16) | (0x7U << 28);
                params.deviceType = 1; /* CUDA */
                params.device = ctx;
                params.apiVersion = api_ver;
                if (open_ex(&params, &enc) == 0 && enc) {
                    if (get_guid_cnt(enc, &guid_cnt) == 0) {
                        printf("NVENC API v%u.%u: session active, codec GUID count=%u\n",
                               maj, min, guid_cnt);
                    }
                }
            }
        }
        dlclose(h_enc);
    } else {
        fprintf(stderr, "NVENC unavailable: %s\n", dlerror());
    }

    p_cuMemFree(dptr);
    p_cuCtxDestroy(ctx);
    dlclose(h_cuda);
    return 0;
}
