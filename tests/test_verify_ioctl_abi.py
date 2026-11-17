import tempfile
import unittest
from pathlib import Path

import sys
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "open-r580-driver"))

import verify_ioctl_abi as via


class TestVerifyIoctlAbi(unittest.TestCase):
    def test_parse_ioctl_headers_synthetic_tree(self):
        with tempfile.TemporaryDirectory() as td:
            kdir = Path(td)
            (kdir / "common" / "inc").mkdir(parents=True)
            (kdir / "nvidia-drm").mkdir()
            (kdir / "conftest").mkdir()

            lines = ["#define NV_IOCTL_MAGIC 'F'\n", "#define NV_IOCTL_BASE 200\n"]
            for idx, name in enumerate(via.REQUIRED_NV_ESCAPES):
                lines.append(f"#define {name} (NV_IOCTL_BASE + {idx})\n")
            (kdir / "common" / "inc" / "nv-ioctl-numbers.h").write_text("".join(lines), encoding="utf-8")
            (kdir / "common" / "inc" / "nv-ioctl-numa.h").write_text(
                "#define NV_ESC_NUMA_INFO (NV_IOCTL_BASE + 15)\n", encoding="utf-8"
            )
            (kdir / "nvidia-drm" / "nvidia-drm-ioctl.h").write_text(
                "#define DRM_IOCTL_NVIDIA_GET_DEV_INFO 1\n", encoding="utf-8"
            )
            (kdir / "conftest" / "functions.h").write_text("#define NV_FOO_PRESENT 1\n#undef NV_BAR\n", encoding="utf-8")

            res = via.parse_ioctl_headers(kdir)
            self.assertEqual(res["ioctl_magic"], "F")
            self.assertEqual(res["escapes"]["NV_ESC_EXPORT_TO_DMABUF_FD"], 10)
            self.assertEqual(res["escapes"]["NV_ESC_NUMA_INFO"], 15)
            self.assertEqual(res["drm_ioctls"], ["DRM_IOCTL_NVIDIA_GET_DEV_INFO"])
            self.assertEqual(res["conftest_counts"]["functions.h"], {"define": 1, "undef": 1})


if __name__ == "__main__":
    unittest.main()

