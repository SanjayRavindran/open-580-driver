// SPDX-License-Identifier: GPL-2.0
/*
 * p4_cleanroom_diag.c - Clean-room read-only telemetry observer for NVIDIA Tesla P4 (GP104GL).
 *
 * Inspects PCI identity and core GP104 BAR0 registers via debugfs without binding
 * PCI ownership or disturbing an active nouveau / nvidia driver.
 */

#include <linux/debugfs.h>
#include <linux/io.h>
#include <linux/module.h>
#include <linux/pci.h>
#include <linux/seq_file.h>
#include <linux/slab.h>
#include <linux/uaccess.h>

#define NV_PCI_VENDOR_NVIDIA      0x10de
#define NV_PCI_DEVICE_TESLA_P4    0x1bb3

/* Verified GP104 BAR0 offsets (all <= 0x42_0000 inside 16 MiB BAR0) */
#define NV_PMC_BOOT_0             0x00000000U
#define NV_PMC_BOOT_42            0x00000a00U
#define NV_PBUS_VBIOS_SCRATCH     0x00001400U
#define NV_FUSE_BASE              0x00021000U
#define NV_FUSE_PRIV_BASE         0x00021400U
#define NV_PFB_LOCAL_MEM_RANGE    0x00100ce0U
#define NV_PFB_WPR_INFO           0x00100e00U
#define NV_PFB_WPR_PRIV_BASE      0x00100e40U
#define NV_PROM_DATA              0x00300000U
#define NV_PROM_DUMP_BYTES        (256U * 1024U)

#define GP104_FALCON_PMU_BASE     0x0010a000U
#define GP104_FALCON_SEC2_BASE    0x00087000U
#define GP104_FALCON_NVDEC_BASE   0x00084000U
#define GP104_FALCON_NVENC0_BASE  0x001c8000U
#define GP104_FALCON_FECS_BASE    0x00409000U
#define GP104_FALCON_GPCCS_BASE   0x0041a000U

#define FALCON_MAILBOX0_OFF       0x040U
#define FALCON_MAILBOX1_OFF       0x044U
#define FALCON_CPUCTL_OFF         0x100U
#define FALCON_BOOTVEC_OFF        0x104U
#define FALCON_HWCFG_OFF          0x108U
#define FALCON_DMACTL_OFF         0x10cU
#define FALCON_SCTL_OFF           0x240U

#define TELEMETRY_MAP_WINDOW      0x00420000UL

static struct dentry *diag_dir;

struct falcon_entry {
	const char *name;
	u32 base;
};

static const struct falcon_entry gp104_falcons[] = {
	{ "PMU",    GP104_FALCON_PMU_BASE },
	{ "SEC2",   GP104_FALCON_SEC2_BASE },
	{ "NVDEC",  GP104_FALCON_NVDEC_BASE },
	{ "NVENC0", GP104_FALCON_NVENC0_BASE },
	{ "FECS",   GP104_FALCON_FECS_BASE },
	{ "GPCCS",  GP104_FALCON_GPCCS_BASE },
};

static void dump_falcon_regs(struct seq_file *m, void __iomem *bar0,
			     const struct falcon_entry *fe)
{
	u32 cpuctl = readl(bar0 + fe->base + FALCON_CPUCTL_OFF);
	u32 dmactl = readl(bar0 + fe->base + FALCON_DMACTL_OFF);
	u32 hwcfg = readl(bar0 + fe->base + FALCON_HWCFG_OFF);
	u32 mb0 = readl(bar0 + fe->base + FALCON_MAILBOX0_OFF);
	u32 mb1 = readl(bar0 + fe->base + FALCON_MAILBOX1_OFF);
	u32 bootvec = readl(bar0 + fe->base + FALCON_BOOTVEC_OFF);
	u32 sctl = readl(bar0 + fe->base + FALCON_SCTL_OFF);

	seq_printf(m,
		   "falcon.%-6s base=0x%06x cpuctl=0x%08x dmactl=0x%08x hwcfg=0x%08x bootvec=0x%08x sctl=0x%08x mb0=0x%08x mb1=0x%08x\n",
		   fe->name, fe->base, cpuctl, dmactl, hwcfg, bootvec, sctl, mb0, mb1);
}

static struct pci_dev *find_ready_p4(u16 *pci_cmd_out, u16 *lnksta_out,
				     resource_size_t *bar0_start, resource_size_t *bar0_len)
{
	struct pci_dev *pdev = pci_get_device(NV_PCI_VENDOR_NVIDIA, NV_PCI_DEVICE_TESLA_P4, NULL);

	if (!pdev)
		return NULL;
	*bar0_start = pci_resource_start(pdev, 0);
	*bar0_len = pci_resource_len(pdev, 0);
	pci_read_config_word(pdev, PCI_COMMAND, pci_cmd_out);
	pcie_capability_read_word(pdev, PCI_EXP_LNKSTA, lnksta_out);
	return pdev;
}

static int p4_telemetry_show(struct seq_file *m, void *v)
{
	resource_size_t bar0_start = 0, bar0_len = 0;
	u16 pci_cmd = 0, lnksta = 0;
	struct pci_dev *pdev = find_ready_p4(&pci_cmd, &lnksta, &bar0_start, &bar0_len);
	void __iomem *bar0;
	u32 boot0, boot42, lmem, wpr_info;
	size_t i;

	if (!pdev) {
		seq_puts(m, "device=not_found (10de:1bb3)\n");
		return 0;
	}

	seq_printf(m, "pci=%s vendor=0x%04x device=0x%04x subsys=0x%04x:0x%04x bar0=0x%llx len=0x%llx cmd=0x%04x lnk_speed=%u lnk_width=x%u\n",
		   pci_name(pdev), pdev->vendor, pdev->device,
		   pdev->subsystem_vendor, pdev->subsystem_device,
		   (unsigned long long)bar0_start, (unsigned long long)bar0_len,
		   pci_cmd, lnksta & PCI_EXP_LNKSTA_CLS,
		   (lnksta & PCI_EXP_LNKSTA_NLW) >> PCI_EXP_LNKSTA_NLW_SHIFT);

	if (pdev->current_state != PCI_D0 || !(pci_cmd & PCI_COMMAND_MEMORY) ||
	    bar0_len < TELEMETRY_MAP_WINDOW) {
		seq_puts(m, "mmio=skipped (device not in D0, PCI memory decode off, or BAR0 too small)\n");
		pci_dev_put(pdev);
		return 0;
	}

	bar0 = ioremap(bar0_start, TELEMETRY_MAP_WINDOW);
	if (!bar0) {
		seq_puts(m, "mmio=ioremap_failed\n");
		pci_dev_put(pdev);
		return 0;
	}

	boot0 = readl(bar0 + NV_PMC_BOOT_0);
	boot42 = readl(bar0 + NV_PMC_BOOT_42);
	lmem = readl(bar0 + NV_PFB_LOCAL_MEM_RANGE);
	wpr_info = readl(bar0 + NV_PFB_WPR_INFO);

	seq_printf(m, "NV_PMC_BOOT_0=0x%08x NV_PMC_BOOT_42=0x%08x (arch=0x%x impl=0x%x)\n",
		   boot0, boot42, (boot42 >> 24) & 0x3fU, (boot42 >> 20) & 0x0fU);
	seq_printf(m, "NV_PFB_LOCAL_MEM_RANGE=0x%08x NV_PFB_WPR_INFO=0x%08x\n",
		   lmem, wpr_info);
	seq_printf(m, "WPR_PRIV[0x100e40..50]=%08x %08x %08x %08x %08x\n",
		   readl(bar0 + NV_PFB_WPR_PRIV_BASE + 0x00),
		   readl(bar0 + NV_PFB_WPR_PRIV_BASE + 0x04),
		   readl(bar0 + NV_PFB_WPR_PRIV_BASE + 0x08),
		   readl(bar0 + NV_PFB_WPR_PRIV_BASE + 0x0c),
		   readl(bar0 + NV_PFB_WPR_PRIV_BASE + 0x10));
	seq_printf(m, "VBIOS_SCRATCH[0x1400..10]=%08x %08x %08x %08x\n",
		   readl(bar0 + NV_PBUS_VBIOS_SCRATCH + 0x00),
		   readl(bar0 + NV_PBUS_VBIOS_SCRATCH + 0x04),
		   readl(bar0 + NV_PBUS_VBIOS_SCRATCH + 0x08),
		   readl(bar0 + NV_PBUS_VBIOS_SCRATCH + 0x0c));
	seq_printf(m, "NV_FUSE[0x21000,0x21400]=%08x %08x\n",
		   readl(bar0 + NV_FUSE_BASE), readl(bar0 + NV_FUSE_PRIV_BASE));

	for (i = 0; i < ARRAY_SIZE(gp104_falcons); i++)
		dump_falcon_regs(m, bar0, &gp104_falcons[i]);

	iounmap(bar0);
	pci_dev_put(pdev);
	return 0;
}
DEFINE_SHOW_ATTRIBUTE(p4_telemetry);

static ssize_t p4_prom_vbios_read(struct file *file, char __user *buf,
				  size_t count, loff_t *ppos)
{
	resource_size_t bar0_start = 0, bar0_len = 0;
	u16 pci_cmd = 0, lnksta = 0;
	struct pci_dev *pdev;
	void __iomem *prom;
	u8 *tmp;
	size_t chunk, i;
	ssize_t ret;

	if (*ppos < 0 || *ppos >= NV_PROM_DUMP_BYTES)
		return 0;
	chunk = min_t(size_t, count, NV_PROM_DUMP_BYTES - *ppos);

	pdev = find_ready_p4(&pci_cmd, &lnksta, &bar0_start, &bar0_len);
	if (!pdev)
		return -ENODEV;
	if (pdev->current_state != PCI_D0 || !(pci_cmd & PCI_COMMAND_MEMORY) ||
	    bar0_len < NV_PROM_DATA + NV_PROM_DUMP_BYTES) {
		pci_dev_put(pdev);
		return -EIO;
	}

	prom = ioremap(bar0_start + NV_PROM_DATA, NV_PROM_DUMP_BYTES);
	if (!prom) {
		pci_dev_put(pdev);
		return -ENOMEM;
	}

	tmp = kmalloc(chunk, GFP_KERNEL);
	if (!tmp) {
		iounmap(prom);
		pci_dev_put(pdev);
		return -ENOMEM;
	}

	/* Read SPI PROM safely in 32-bit aligned MMIO cycles */
	for (i = 0; i < chunk; i++) {
		size_t abs_off = (size_t)*ppos + i;
		u32 w = readl(prom + (abs_off & ~3UL));

		tmp[i] = (u8)((w >> ((abs_off & 3UL) * 8U)) & 0xffU);
	}

	ret = copy_to_user(buf, tmp, chunk) ? -EFAULT : (ssize_t)chunk;
	if (ret > 0)
		*ppos += ret;

	kfree(tmp);
	iounmap(prom);
	pci_dev_put(pdev);
	return ret;
}

static const struct file_operations p4_prom_vbios_fops = {
	.owner = THIS_MODULE,
	.read = p4_prom_vbios_read,
	.llseek = default_llseek,
};

static int __init p4_cleanroom_diag_init(void)
{
	diag_dir = debugfs_create_dir("p4_cleanroom_diag", NULL);
	if (IS_ERR(diag_dir))
		return PTR_ERR(diag_dir);

	debugfs_create_file("telemetry", 0444, diag_dir, NULL, &p4_telemetry_fops);
	debugfs_create_file("prom_vbios", 0444, diag_dir, NULL, &p4_prom_vbios_fops);
	pr_info("p4_cleanroom_diag: loaded read-only GP104 debugfs telemetry + PROM reader\n");
	return 0;
}

static void __exit p4_cleanroom_diag_exit(void)
{
	debugfs_remove_recursive(diag_dir);
	pr_info("p4_cleanroom_diag: unloaded\n");
}

module_init(p4_cleanroom_diag_init);
module_exit(p4_cleanroom_diag_exit);

MODULE_LICENSE("GPL");
MODULE_AUTHOR("Clean-Room GP104 Research");
MODULE_DESCRIPTION("Read-only GP104 (Tesla P4) PCI/BAR0/Falcon diagnostic telemetry");
