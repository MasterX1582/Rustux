#pragma once
/* dma-mapping shim: only the error sentinel kfifo.c/h needs. */
#include <linux/types.h>

#define DMA_MAPPING_ERROR (~(dma_addr_t)0)
