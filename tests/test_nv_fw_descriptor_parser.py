import struct
import unittest
from pathlib import Path

import sys
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "tools"))

import nv_fw_descriptor_parser as dp


class TestNvFwDescriptorParser(unittest.TestCase):
    def test_lsf_xor_footer_decodes_real_and_synthetic_fids(self):
        for fid, expected_name in [(0, "PMU"), (2, "FECS"), (3, "GPCCS"), (4, "NVENC"), (7, "SEC2")]:
            buf = bytearray(192)
            struct.pack_into(
                "<4I",
                buf,
                176,
                dp.LSF_XOR_FOOTER[0] ^ fid,
                dp.LSF_XOR_FOOTER[1],
                dp.LSF_XOR_FOOTER[2],
                dp.LSF_XOR_FOOTER[3],
            )
            parsed = dp.parse_lsf_sig_wrapper(bytes(buf))
            self.assertEqual(parsed.falcon_id, fid)
            self.assertEqual(parsed.falcon_name, expected_name)

    def test_prom_jffs_synthetic_reassembly(self):
        prom = bytearray(0x400)
        prom[0x000:0x00C] = struct.pack("<4sII", b"JFFS", 2, 0x300)
        # Write 2 chunks for OBD object across 128B blocks
        part1 = b"Tesla P4"
        part2 = b"-0200"
        struct.pack_into("<4sIII", prom, 0x080, b"\x02OBD", (len(part1) << 16) | 0, 1, 0)
        prom[0x090:0x090 + len(part1)] = part1
        struct.pack_into("<4sIII", prom, 0x100, b"\x02OBD", (len(part2) << 16) | len(part1), 1, 0)
        prom[0x110:0x110 + len(part2)] = part2
        res = dp.parse_prom_inforom(bytes(prom))
        self.assertEqual(res["version"], 2)
        self.assertEqual(res["records"][0]["name"], "OBD")
        self.assertIn("Tesla P4-0200", res["records"][0]["ascii_strings"])

    def test_real_extracted_gp104_bundle_if_present(self):
        blob_root = Path(__file__).resolve().parents[1] / "firmware-blobs" / "gp104"
        if not (blob_root / "pmu" / "gp104_pmu_ls_sig.bin").exists():
            self.skipTest("Isolated GP104 firmware-blobs bundle not extracted yet")
        report = dp.inspect_gp104_blob_directory(blob_root)
        fids = {item["file"]: item["falcon_id"] for item in report["ls_signatures"]}
        self.assertEqual(fids["pmu/gp104_pmu_ls_sig.bin"], 0)
        self.assertEqual(fids["gr/gp104_fecs_ls_sig.bin"], 2)
        self.assertEqual(fids["gr/gp104_gpccs_ls_sig.bin"], 3)
        self.assertEqual(fids["nvenc/gp104_nvenc_ls_sig.bin"], 4)
        self.assertEqual(fids["sec2/gp104_sec2_rtos/id_0x27.bin"], 7)

        pmu_rtos = report["descriptors"]["pmu/gp104_pmu_rtos/id_0x12.bin"]
        self.assertEqual(pmu_rtos["app_entry"], 0x5E00)
        self.assertEqual(pmu_rtos["total_expected_image_bytes"], 300288)
        sec2_rtos = report["descriptors"]["sec2/gp104_sec2_rtos/id_0x12.bin"]
        self.assertEqual(sec2_rtos["app_entry"], 0xFD00)
        self.assertEqual(sec2_rtos["total_expected_image_bytes"], 221440)

        vb = report["vbios"]
        self.assertEqual(vb["efi_gap_bytes"], 0x10A00)
        self.assertEqual(len(vb["apps"]), 3)

        prom = report["prom_inforom"]
        names = {r["name"]: r for r in prom["records"]}
        for required in ("ROM", "IMG", "OBD", "OEM", "ECC", "PBL", "BBX"):
            self.assertIn(required, names)
        self.assertTrue(any("Tesla P4" in s for s in names["OBD"]["ascii_strings"]))


if __name__ == "__main__":
    unittest.main()
