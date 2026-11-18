#pragma once
/* scatterlist model for the kfifo DMA harness.
 *
 * Upstream packs a struct page pointer plus chain/end flags into
 * page_link; the __kfifo_* code only ever calls sg_set_buf() (set the
 * virtual address + length of one entry) and assigns through the
 * sg_dma_address()/sg_dma_len() lvalue macros, so this model keeps a
 * plain void *buf for the virtual address and explicit dma fields.
 * x86 selects NEED_SG_DMA_LENGTH, so sg_dma_len maps to dma_length
 * (a distinct field), not to length.
 *
 * A real driver walks sgl[0..n): the sg entries point into the backing
 * buffer of the fifo, which is exactly what setup_sgl() produces here. */
#include <string.h>
#include <linux/types.h>
#include <linux/bug.h>

struct scatterlist {
	void		*buf;		/* sg_set_buf target (modelled page) */
	unsigned int	offset;
	unsigned int	length;
	dma_addr_t	dma_address;
	unsigned int	dma_length;	/* CONFIG_NEED_SG_DMA_LENGTH */
	int		is_end;		/* sg_mark_end() marker */
};

#define sg_dma_address(sg)	((sg)->dma_address)
#define sg_dma_len(sg)		((sg)->dma_length)

static inline void sg_init_table(struct scatterlist *sgl, unsigned int nents)
{
	memset(sgl, 0, nents * sizeof(*sgl));
}

static inline void sg_mark_end(struct scatterlist *sg)
{
	sg->is_end = 1;
}

static inline int sg_is_last(struct scatterlist *sg)
{
	return sg->is_end;
}

/* No chain tables in this model: entries live in one flat array. */
static inline struct scatterlist *sg_next(struct scatterlist *sg)
{
	return sg_is_last(sg) ? NULL : sg + 1;
}

static inline void sg_set_buf(struct scatterlist *sg, const void *buf,
			      unsigned int buflen)
{
	sg->buf = (void *)buf;
	sg->offset = 0;
	sg->length = buflen;
}
