#pragma once
/* asm/page.h shim for the lib-genradix-interval harness.
 * generic-radix-tree.h includes this for context only; the unit uses
 * GENRADIX_NODE_SHIFT (9) rather than PAGE_SHIFT, so only the usual
 * constants are provided for completeness. */
#define PAGE_SHIFT 12
#define PAGE_SIZE (1UL << PAGE_SHIFT)
