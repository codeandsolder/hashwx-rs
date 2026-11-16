// SPDX-License-Identifier: GPL-2.0-only
/*
 * One privileged, exclusive F1 AppPF control + coherent DMA allocation.
 *
 * Device-bound DMA allocation returns the bus address for this EXACT PCI
 * requester (including IOMMU translation). Userspace receives a coherent
 * 4 KiB mmap, the 64-bit bus address, and checked BAR0 32-bit ioctls.
 *
 * Unbind waits for existing users and VMA references to close, then revokes
 * bus mastering before freeing the DMA mapping. Users must drain/disarm DMA
 * before closing; this driver keeps the DMA memory alive until unplug.
 */
#include <linux/atomic.h>
#include <linux/dma-mapping.h>
#include <linux/fs.h>
#include <linux/io.h>
#include <linux/miscdevice.h>
#include <linux/module.h>
#include <linux/mutex.h>
#include <linux/pci.h>
#include <linux/slab.h>
#include <linux/uaccess.h>
#include <linux/wait.h>

#include "hashwx_f1_uapi.h"

#define AWS_VENDOR 0x1d0f

struct hashwx_f1_dev {
    struct pci_dev *pci;
    struct miscdevice misc;
    struct mutex lock;
    wait_queue_head_t wait;
    atomic_t opened;
    atomic_t mappings;
    bool removed;
    void __iomem *regs;
    void *cpu;
    dma_addr_t dma;
    char node_name[64];
};

static int hashwx_open(struct inode *inode, struct file *file)
{
    struct hashwx_f1_dev *dev =
        container_of(file->private_data, struct hashwx_f1_dev, misc);
    int rc = 0;

    mutex_lock(&dev->lock);
    if (dev->removed)
        rc = -ENODEV;
    else if (atomic_cmpxchg(&dev->opened, 0, 1) != 0)
        rc = -EBUSY;
    else {
        file->private_data = dev;
        nonseekable_open(inode, file);
    }
    mutex_unlock(&dev->lock);
    return rc;
}

static int hashwx_release(struct inode *inode, struct file *file)
{
    struct hashwx_f1_dev *dev = file->private_data;

    atomic_set(&dev->opened, 0);
    wake_up_all(&dev->wait);
    return 0;
}

static long hashwx_ioctl(struct file *file, unsigned int cmd,
                         unsigned long arg)
{
    struct hashwx_f1_dev *dev = file->private_data;
    struct hashwx_f1_register reg;
    struct hashwx_f1_info info;
    void __user *uarg = (void __user *)arg;
    long rc = 0;

    mutex_lock(&dev->lock);
    if (dev->removed) {
        rc = -ENODEV;
        goto out;
    }
    switch (cmd) {
    case HASHWX_F1_GET_INFO:
        info.dma_bus_address = (u64)dev->dma;
        info.dma_size = HASHWX_F1_DMA_BYTES;
        info.abi_version = HASHWX_F1_IOCTL_ABI;
        if (copy_to_user(uarg, &info, sizeof(info)))
            rc = -EFAULT;
        break;
    case HASHWX_F1_PEEK:
    case HASHWX_F1_POKE:
        if (copy_from_user(&reg, uarg, sizeof(reg))) {
            rc = -EFAULT;
            break;
        }
        if (reg.offset >= HASHWX_F1_MMIO_BYTES ||
            (reg.offset & 3) != 0) {
            rc = -EINVAL;
            break;
        }
        if (cmd == HASHWX_F1_POKE)
            iowrite32(reg.value, (u8 __iomem *)dev->regs + reg.offset);
        else {
            reg.value = ioread32((u8 __iomem *)dev->regs + reg.offset);
            if (copy_to_user(uarg, &reg, sizeof(reg)))
                rc = -EFAULT;
        }
        break;
    default:
        rc = -ENOTTY;
    }
out:
    mutex_unlock(&dev->lock);
    return rc;
}

static void hashwx_vma_open(struct vm_area_struct *vma)
{
    struct hashwx_f1_dev *dev = vma->vm_private_data;

    atomic_inc(&dev->mappings);
}

static void hashwx_vma_close(struct vm_area_struct *vma)
{
    struct hashwx_f1_dev *dev = vma->vm_private_data;

    atomic_dec(&dev->mappings);
    wake_up_all(&dev->wait);
}

static const struct vm_operations_struct hashwx_vm_ops = {
    .open = hashwx_vma_open,
    .close = hashwx_vma_close,
};

static int hashwx_mmap(struct file *file, struct vm_area_struct *vma)
{
    struct hashwx_f1_dev *dev = file->private_data;
    size_t length = vma->vm_end - vma->vm_start;
    int rc;

    if (vma->vm_pgoff != 0 || length != HASHWX_F1_DMA_BYTES)
        return -EINVAL;

    mutex_lock(&dev->lock);
    if (dev->removed)
        rc = -ENODEV;
    else {
        rc = dma_mmap_coherent(&dev->pci->dev, vma, dev->cpu,
                               dev->dma, HASHWX_F1_DMA_BYTES);
        if (!rc) {
            vma->vm_ops = &hashwx_vm_ops;
            vma->vm_private_data = dev;
            hashwx_vma_open(vma);
        }
    }
    mutex_unlock(&dev->lock);
    return rc;
}

static const struct file_operations hashwx_fops = {
    .owner = THIS_MODULE,
    .open = hashwx_open,
    .release = hashwx_release,
    .unlocked_ioctl = hashwx_ioctl,
    .compat_ioctl = compat_ptr_ioctl,
    .mmap = hashwx_mmap,
};

static int hashwx_probe(struct pci_dev *pdev,
                        const struct pci_device_id *id)
{
    struct hashwx_f1_dev *dev;
    int rc;

    dev = kzalloc(sizeof(*dev), GFP_KERNEL);
    if (!dev)
        return -ENOMEM;

    dev->pci = pdev;
    mutex_init(&dev->lock);
    init_waitqueue_head(&dev->wait);
    atomic_set(&dev->opened, 0);
    atomic_set(&dev->mappings, 0);

    rc = pci_enable_device_mem(pdev);
    if (rc)
        goto fail_dev;

    rc = pci_request_region(pdev, 0, "hashwx-f1");
    if (rc)
        goto fail_enable;

    if (pci_resource_len(pdev, 0) < HASHWX_F1_MMIO_BYTES) {
        rc = -ENODEV;
        goto fail_region;
    }

    rc = dma_set_mask_and_coherent(&pdev->dev, DMA_BIT_MASK(64));
    if (rc)
        goto fail_region;

    dev->regs = pci_iomap_range(pdev, 0, 0, HASHWX_F1_MMIO_BYTES);
    if (!dev->regs) {
        rc = -ENOMEM;
        goto fail_region;
    }

    dev->cpu = dma_alloc_coherent(&pdev->dev, HASHWX_F1_DMA_BYTES,
                                  &dev->dma, GFP_KERNEL);
    if (!dev->cpu) {
        rc = -ENOMEM;
        goto fail_iomap;
    }
    memset(dev->cpu, 0, HASHWX_F1_DMA_BYTES);
    pci_set_master(pdev);

    snprintf(dev->node_name, sizeof(dev->node_name),
             "hashwx-f1-%s", pci_name(pdev));
    dev->misc.minor = MISC_DYNAMIC_MINOR;
    dev->misc.name = dev->node_name;
    dev->misc.fops = &hashwx_fops;
    dev->misc.parent = &pdev->dev;
    dev->misc.mode = 0600;

    pci_set_drvdata(pdev, dev);
    rc = misc_register(&dev->misc);
    if (!rc) {
        dev_info(&pdev->dev, "F1 coherent host DMA ready (%s, %pad)\n",
                 dev->node_name, &dev->dma);
        return 0;
    }

    pci_clear_master(pdev);
    pci_set_drvdata(pdev, NULL);
    dma_free_coherent(&pdev->dev, HASHWX_F1_DMA_BYTES,
                      dev->cpu, dev->dma);
fail_iomap:
    pci_iounmap(pdev, dev->regs);
fail_region:
    pci_release_region(pdev, 0);
fail_enable:
    pci_disable_device(pdev);
fail_dev:
    kfree(dev);
    return rc;
}

static void hashwx_remove(struct pci_dev *pdev)
{
    struct hashwx_f1_dev *dev = pci_get_drvdata(pdev);

    mutex_lock(&dev->lock);
    dev->removed = true;
    mutex_unlock(&dev->lock);
    misc_deregister(&dev->misc);
    /*
     * All user mappings must be gone before releasing the DMA allocation.
     * Hot-unplug/unbind deliberately waits rather than freeing live memory.
     */
    wait_event(dev->wait, !atomic_read(&dev->opened) &&
               !atomic_read(&dev->mappings));

    pci_clear_master(pdev);
    dma_free_coherent(&pdev->dev, HASHWX_F1_DMA_BYTES,
                      dev->cpu, dev->dma);
    pci_iounmap(pdev, dev->regs);
    pci_release_region(pdev, 0);
    pci_disable_device(pdev);
    pci_set_drvdata(pdev, NULL);
    kfree(dev);
}

static const struct pci_device_id hashwx_ids[] = {
    { PCI_DEVICE(AWS_VENDOR, 0xf000) }, /* cl_hello_world */
    { PCI_DEVICE(AWS_VENDOR, 0xf001) },
    { PCI_DEVICE(AWS_VENDOR, 0xf002) },
    { PCI_DEVICE(AWS_VENDOR, 0xf003) },
    { }
};
MODULE_DEVICE_TABLE(pci, hashwx_ids);

static struct pci_driver hashwx_pci_driver = {
    .name = "hashwx_f1",
    .id_table = hashwx_ids,
    .probe = hashwx_probe,
    .remove = hashwx_remove,
};
module_pci_driver(hashwx_pci_driver);

MODULE_AUTHOR("codeandsolder");
MODULE_DESCRIPTION("AWS F1 AppPF coherent DMA ring and BAR0 control driver");
MODULE_LICENSE("GPL");
