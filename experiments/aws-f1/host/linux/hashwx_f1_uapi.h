/* F1 PCI kernel driver userspace ABI, Linux only.
 * SPDX-License-Identifier: GPL-2.0 WITH Linux-syscall-note
 */
#ifndef HASHWX_F1_UAPI_H
#define HASHWX_F1_UAPI_H

#include <linux/ioctl.h>
#include <linux/types.h>

#define HASHWX_F1_IOCTL_ABI 1
#define HASHWX_F1_DMA_BYTES 4096U
#define HASHWX_F1_MMIO_BYTES 0x6000U

struct hashwx_f1_info {
    __u64 dma_bus_address;
    __u32 dma_size;
    __u32 abi_version;
};

struct hashwx_f1_register {
    __u32 offset;
    __u32 value;
};

#define HASHWX_F1_GET_INFO _IOR('H', 0x70, struct hashwx_f1_info)
#define HASHWX_F1_PEEK _IOWR('H', 0x71, struct hashwx_f1_register)
#define HASHWX_F1_POKE _IOW('H', 0x72, struct hashwx_f1_register)

#endif
