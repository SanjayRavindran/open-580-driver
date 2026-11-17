#define _GNU_SOURCE
#include <dlfcn.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

typedef int CUresult;
typedef int CUdevice;
typedef void *CUcontext;

typedef int (*pfn_cuInit)(unsigned int flags);
typedef CUresult (*pfn_cuDeviceGet)(CUdevice *dev, int ordinal);
typedef CUresult (*pfn_cuDeviceGetName)(char *name, int len, CUdevice dev);
typedef CUresult (*pfn_cuCtxCreate)(CUcontext *pctx, unsigned int flags, CUdevice dev);
typedef CUresult (*pfn_cuCtxDestroy)(CUcontext ctx);

/* Minimal binary-compatible ABI subset of CUDA NVDEC (nvcuvid.h / cuviddec.h). */
typedef enum {
    NV_CODEC_MPEG2 = 1,
    NV_CODEC_VC1   = 3,
    NV_CODEC_H264  = 4,
    NV_CODEC_JPEG  = 5,
    NV_CODEC_HEVC  = 8,
    NV_CODEC_VP8   = 9,
    NV_CODEC_VP9   = 10,
    NV_CODEC_AV1   = 11
} nv_video_codec_t;

typedef struct {
    int           eCodecType;
    int           eChromaFormat; /* 1 = 4:2:0 */
    unsigned int  nBitDepthMinus8;
    unsigned int  reserved1[3];
    unsigned char bIsSupported;
    unsigned char nNumNVDECs;
    unsigned short nOutputFormatMask;
    unsigned int  nMaxWidth;
    unsigned int  nMaxHeight;
    unsigned int  nMaxMBCount;
    unsigned short nMinWidth;
    unsigned short nMinHeight;
    unsigned char bIsHistogramSupported;
    unsigned char reserved2[3];
    unsigned int  reserved3[16]; /* oversize zero tail protects against future minor header growth */
} nv_cuvid_decode_caps_t;

typedef CUresult (*pfn_cuvidGetDecoderCaps)(nv_cuvid_decode_caps_t *pdc);

static int check_caps(pfn_cuvidGetDecoderCaps fn, const char *label, int codec, unsigned int bitdepth_minus8) {
    nv_cuvid_decode_caps_t caps;
    memset(&caps, 0, sizeof(caps));
    caps.eCodecType = codec;
    caps.eChromaFormat = 1; /* 4:2:0 */
    caps.nBitDepthMinus8 = bitdepth_minus8;
    CUresult rc = fn(&caps);
    printf("  %-14s rc=%d supported=%u nvdecs=%u min=%ux%u max=%ux%u max_mb=%u out_mask=0x%04x\n",
           label, rc, (unsigned)caps.bIsSupported, (unsigned)caps.nNumNVDECs,
           (unsigned)caps.nMinWidth, (unsigned)caps.nMinHeight,
           caps.nMaxWidth, caps.nMaxHeight, caps.nMaxMBCount, (unsigned)caps.nOutputFormatMask);
    return (rc == 0) ? (int)caps.bIsSupported : -1;
}

int main(void) {
    void *hcuda = dlopen("libcuda.so.1", RTLD_NOW | RTLD_GLOBAL);
    void *hcuvid = dlopen("libnvcuvid.so.1", RTLD_NOW);
    if (!hcuda || !hcuvid) {
        fprintf(stderr, "dlopen failed: cuda=%p cuvid=%p (%s)\n", hcuda, hcuvid, dlerror());
        return 1;
    }

    pfn_cuInit p_cuInit = (pfn_cuInit)dlsym(hcuda, "cuInit");
    pfn_cuDeviceGet p_cuDeviceGet = (pfn_cuDeviceGet)dlsym(hcuda, "cuDeviceGet");
    pfn_cuDeviceGetName p_cuDeviceGetName = (pfn_cuDeviceGetName)dlsym(hcuda, "cuDeviceGetName");
    pfn_cuCtxCreate p_cuCtxCreate = (pfn_cuCtxCreate)dlsym(hcuda, "cuCtxCreate_v2");
    if (!p_cuCtxCreate) p_cuCtxCreate = (pfn_cuCtxCreate)dlsym(hcuda, "cuCtxCreate");
    pfn_cuCtxDestroy p_cuCtxDestroy = (pfn_cuCtxDestroy)dlsym(hcuda, "cuCtxDestroy_v2");
    pfn_cuvidGetDecoderCaps p_caps = (pfn_cuvidGetDecoderCaps)dlsym(hcuvid, "cuvidGetDecoderCaps");

    if (!p_cuInit || !p_cuDeviceGet || !p_cuCtxCreate || !p_caps) {
        fprintf(stderr, "Missing CUDA/CUVID entrypoints\n");
        return 2;
    }

    CUdevice dev = 0;
    CUcontext ctx = NULL;
    char name[128] = {0};
    if (p_cuInit(0) != 0 || p_cuDeviceGet(&dev, 0) != 0 || p_cuCtxCreate(&ctx, 0, dev) != 0) {
        fprintf(stderr, "Failed to initialize CUDA context on device 0\n");
        return 3;
    }
    if (p_cuDeviceGetName) p_cuDeviceGetName(name, sizeof(name), dev);
    printf("NVDEC (CUVID) hardware decoder capabilities on %s:\n", name[0] ? name : "GPU 0");

    int h264 = check_caps(p_caps, "H.264 8-bit", NV_CODEC_H264, 0);
    int hevc8 = check_caps(p_caps, "HEVC 8-bit", NV_CODEC_HEVC, 0);
    int hevc10 = check_caps(p_caps, "HEVC 10-bit", NV_CODEC_HEVC, 2);
    int vp9 = check_caps(p_caps, "VP9 8-bit", NV_CODEC_VP9, 0);
    int mpeg2 = check_caps(p_caps, "MPEG2 8-bit", NV_CODEC_MPEG2, 0);
    int jpeg = check_caps(p_caps, "JPEG 8-bit", NV_CODEC_JPEG, 0);
    int av1 = check_caps(p_caps, "AV1 8-bit", NV_CODEC_AV1, 0);

    if (p_cuCtxDestroy && ctx) p_cuCtxDestroy(ctx);
    dlclose(hcuvid);
    dlclose(hcuda);

    /* Pascal GP104 guarantees H264 + HEVC (8/10-bit) + VP9 + MPEG2 + JPEG decode; AV1 is Ampere+ (0). */
    if (h264 != 1 || hevc8 != 1 || hevc10 != 1 || vp9 != 1 || mpeg2 != 1 || jpeg != 1) {
        fprintf(stderr, "ERROR: Expected Pascal GP104 NVDEC codecs missing (h264=%d hevc8=%d vp9=%d)\n", h264, hevc8, vp9);
        return 4;
    }
    printf("NVDEC capability probe PASSED (AV1=%d expected 0 on Pascal GP104).\n", av1);
    return 0;
}

