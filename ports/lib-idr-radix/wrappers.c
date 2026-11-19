/* Scalar wrappers around lib/idr.c, lib/radix-tree.c and lib/xarray.c
 * (the two veneer units are byte-identical upstream copies; xarray.c is
 * the engine both veneers ride on), so the standard scalar bridge can
 * drive them.
 *
 * Address-space model: all three trees store real C pointers, so
 * pointer identity must be foldable.  struct xa_node lives in a static
 * pool of NP objects reached through the kmem_cache shims (xarray.c
 * calls kmem_cache_alloc_lru, radix-tree.c calls kmem_cache_alloc —
 * one pool, one shared alloc-ordinal stream).  IDR/radix user entries
 * are pointers into an aligned u64 entry pool plus tagged pointers and
 * xa_mk_value() integers.  IDA leaves are struct ida_bitmap objects
 * allocated through kzalloc() from a second deterministic pool.
 *
 * efold() maps every entry/node to ONE canonical u64 (also the exact
 * encoding the Rust port uses as its pointer values):
 *   NULL                    -> 0
 *   entry pool &epool[i]    -> EBASE + i*16 (+tag for tagged pointers)
 *   node pool &node_pool[i] -> NBASE + i*4096 (|2 in slot encodings)
 *   bitmap pool &bpool[i]   -> BBASE + i*256
 *   &idr.idr_rt / &ida.xa / &rt -> 0x810000 / 0x820000 / 0x830000
 *   &idr / &ida structs       -> 0x840000 / 0x850000
 *   internal/value/error ents -> the integer itself (already canonical)
 *
 * Slot pointers (void __rcu **) canonicalise via scanon():
 *   &root->xa_head            -> SROOT + root index (0/1/2)
 *   &node_pool[i].slots[o]    -> SBASE + i*64 + o
 *
 * Allocation failure injection: a per-wrapper u64 fail_mask nominates
 * which alloc calls (1..64, counted in alloc_ctr) return NULL — this
 * drives radix_tree_node_alloc -ENOMEM/preload-pop paths, the IDA
 * bitmap kzalloc_obj failures and xas_nomem retries deterministically.
 *
 * WARN semantics: upstream WARN_ON_ONCE uses a process-persistent
 * static, which would leak state between recorded cases; the shim
 * counts every WARN hit in kshim_warn_hits instead (folded at the end
 * of every wrapper), which is what the Rust port mirrors.
 *
 * Every wrapper returns one order-sensitive FNV-1a-style checksum folded
 * over each op's results plus a periodic FULL STATE DUMP: all three
 * roots, every live pool node (scalars, parent/array identity, 64
 * slots, 3 tag words), the preload stack and every live IDA bitmap.
 * Raw addresses never fold.
 */
#include <stdint.h>
#include <stddef.h>
#include <string.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <linux/types.h>
#include <linux/list.h>
#include <linux/container_of.h>
#include <linux/xarray.h>
#include <linux/radix-tree.h>
#include <linux/idr.h>
#include <linux/slab.h>
#include <linux/errno.h>
#include <linux/gfp.h>
#include <linux/printk.h>
#include <linux/bitmap.h>
#include <linux/bug.h>
#include <linux/percpu.h>
#include <linux/local_lock.h>

#include "radix-tree.h"

#define NP 160		/* xa_node pool slots */
#define EP 96		/* user entry objects */
#define NB 48		/* ida_bitmap pool objects */
#define NILX 0xffffu	/* "no such index" fold sentinel */

/* canonical encodings shared with port.rs */
#define NBASE 0x40000ULL	/* node canonical base, stride 0x1000 */
#define NSTRIDE 0x1000ULL
#define EBASE 0x10000ULL	/* entry canonical base, stride 16 */
#define BBASE 0x200000ULL	/* ida_bitmap canonical base, stride 0x100 */
#define RTIDR_CANON 0x810000ULL	/* &idr.idr_rt */
#define IDAX_CANON  0x820000ULL	/* &ida.xa */
#define RRT_CANON   0x830000ULL	/* &rt */
#define IDRP_CANON  0x840000ULL	/* &idr */
#define IDAP_CANON  0x850000ULL	/* &ida */
#define SROOT 0xB00000ULL	/* &root->xa_head slot canonicals */
#define SBASE 0xB10000ULL	/* node slot canonical base, stride 64 */

static struct xa_node node_pool[NP];
static uint8_t node_used[NP];
static struct ida_bitmap bpool[NB];
static uint8_t bmap_used[NB];
static uint64_t epool[EP] __attribute__((aligned(64)));

static struct idr idr;
static struct ida ida;
static struct radix_tree_root rt;

static struct kmem_cache node_cache = { sizeof(struct xa_node) };
unsigned long kshim_warn_hits;

/* ---- deterministic allocator (slab boundary, see linux/slab.h) ---- */

static unsigned long alloc_ctr;
static uint64_t fail_mask;

/* one shared ordinal stream for every allocation the units perform */
static int alloc_fail_check(void)
{
	alloc_ctr++;
	if (alloc_ctr <= 64 && (fail_mask >> (alloc_ctr - 1)) & 1)
		return 1;
	return 0;
}

static struct xa_node *alloc_node(void)
{
	int i;
	if (alloc_fail_check())
		return NULL;
	for (i = 0; i < NP; i++)
		if (!node_used[i]) {
			node_used[i] = 1;
			return &node_pool[i];
		}
	return NULL;
}

void *kmem_cache_alloc_lru(struct kmem_cache *cachep, struct list_lru *lru,
			   gfp_t gfp)
{
	(void)cachep; (void)lru; (void)gfp;
	return alloc_node();
}

void *kmem_cache_alloc(struct kmem_cache *cachep, gfp_t gfp)
{
	(void)cachep; (void)gfp;
	return alloc_node();
}

void kmem_cache_free(struct kmem_cache *cachep, void *obj)
{
	struct xa_node *node = obj;
	(void)cachep;
	node_used[node - node_pool] = 0;
}

struct kmem_cache *kmem_cache_create(const char *name, unsigned int size,
		unsigned int align, unsigned long flags,
		void (*ctor)(void *))
{
	(void)name; (void)size; (void)align; (void)flags; (void)ctor;
	return &node_cache;
}

void kmem_cache_destroy(struct kmem_cache *cachep)
{
	(void)cachep;
}

void *kzalloc(size_t size, gfp_t gfp)
{
	int i;
	BUG_ON(size > sizeof(struct ida_bitmap));
	(void)gfp;
	if (alloc_fail_check())
		return NULL;
	for (i = 0; i < NB; i++)
		if (!bmap_used[i]) {
			bmap_used[i] = 1;
			memset(&bpool[i], 0, sizeof bpool[i]);
			return &bpool[i];
		}
	return NULL;
}

void *kmalloc(size_t size, gfp_t gfp)
{
	return kzalloc(size, gfp);
}

void kfree(const void *ptr)
{
	const struct ida_bitmap *b = ptr;
	if (!ptr)
		return;
	BUG_ON((const void *)b < (const void *)bpool ||
	       (const void *)b >= (const void *)(bpool + NB));
	bmap_used[b - bpool] = 0;
}

/* ---- rng / checksum plumbing (same scheme as the other ports) ---- */

static uint64_t rng_state;
static uint64_t xs64(void)
{
	uint64_t x = rng_state;
	x ^= x >> 12; x ^= x << 25; x ^= x >> 27;
	rng_state = x;
	return x * 2685821657736338717ULL;
}

static uint64_t iabs(int64_t v)
{
	return v < 0 ? 0ULL - (uint64_t)v : (uint64_t)v;
}

static uint64_t g_h;
static int g_trace = -1;
static int trace_on(void)
{
	if (g_trace < 0) {
		const char *e = getenv("KTRACE");
		g_trace = e && *e;
	}
	return g_trace;
}
static void fold(uint64_t v)
{
	if (trace_on())
		fprintf(stderr, "F %016llx\n", (unsigned long long)v);
	g_h = (g_h ^ v) * 0x100000001b3ULL;
}

/* ---- canonicalization ---- */

static int node_idx(const struct xa_node *n)
{
	return (int)(n - node_pool);
}

static uint64_t ncanon(const struct xa_node *n)
{
	return n ? NBASE + (uint64_t)node_idx(n) * NSTRIDE : 0;
}

static uint64_t efold(const void *e)
{
	uintptr_t p = (uintptr_t)e;
	if (!e)
		return 0;
	if (xa_is_node(e)) {
		struct xa_node *n = xa_to_node(e);
		/* xa_is_node() is (internal && > 4096): XA_ERROR() encodings
		 * satisfy it too, and folding them through ncanon() would
		 * subtract node_pool from a fake pointer — ASLR garbage.
		 * Only real pool nodes canonicalise; error/retry/sibling
		 * sentinels fold raw. */
		if ((uintptr_t)n - (uintptr_t)node_pool <
		    sizeof(node_pool))
			return ncanon(n) | 2;
		return p;
	}
	if (p >= (uintptr_t)epool &&
	    p < (uintptr_t)epool + sizeof(epool))
		/* preserve the low tag bits: tagged epool pointers keep
		 * their tag in the canonical encoding (odd => value) */
		return EBASE + ((p - (uintptr_t)epool) & ~7ULL) * 2 +
			(p & 3);
	if (p >= (uintptr_t)bpool &&
	    p < (uintptr_t)bpool + sizeof(bpool))
		return BBASE + (p - (uintptr_t)bpool) /
			sizeof(struct ida_bitmap) * 256;
	return p;	/* internal/value/error entries are canonical */
}

static uint64_t arraycanon(const struct xarray *a)
{
	if (a == &idr.idr_rt)
		return RTIDR_CANON;
	if (a == &ida.xa)
		return IDAX_CANON;
	if (a == &rt)
		return RRT_CANON;
	if (a == (const struct xarray *)1)
		return 1;
	if (!a)
		return 0;
	return 2;
}

/* void __rcu ** slot address -> canonical u64 */
static uint64_t scanon(void __rcu **slot)
{
	uintptr_t p = (uintptr_t)slot;
	uintptr_t off;
	unsigned int ni;
	if (!slot)
		return 0;
	if (slot == (void __rcu **)&idr.idr_rt.xa_head)
		return SROOT + 0;
	if (slot == (void __rcu **)&ida.xa.xa_head)
		return SROOT + 1;
	if (slot == (void __rcu **)&rt.xa_head)
		return SROOT + 2;
	if (p < (uintptr_t)node_pool ||
	    p >= (uintptr_t)node_pool + sizeof(node_pool))
		return 0xdeadbadULL;
	off = p - (uintptr_t)node_pool;
	ni = off / sizeof(struct xa_node);
	off -= ni * sizeof(struct xa_node);
	off -= offsetof(struct xa_node, slots);
	return SBASE + ni * 64 + off / sizeof(void *);
}

/* printk folding: each conversion folds one canonicalised arg */
static void kprintv(const char *fmt, va_list ap)
{
	const char *f;
	for (f = fmt; *f; f++) {
		unsigned long v;
		void *ptr;
		if (*f != '%')
			continue;
		f++;
		if (*f == 's') {
			(void)va_arg(ap, const char *);
			fold(0);
			continue;
		}
		if (*f == 'l')
			f++;
		if (*f == 'x') {	/* %lx */
			if (f[-1] == 'l')
				v = va_arg(ap, unsigned long);
			else
				v = va_arg(ap, unsigned int);
			if (f[1] == 'p') {	/* unreachable; %lx handled */
				f++;
			}
			fold(v);
		} else if (*f == 'u') {
			if (f[-1] == 'l')
				fold(va_arg(ap, unsigned long));
			else
				fold(va_arg(ap, unsigned int));
		} else if (*f == 'd') {
			fold((uint64_t)(int64_t)va_arg(ap, int));
		} else if (*f == 'p') {
			ptr = va_arg(ap, void *);
			if (f[1] == 'x')
				f++;
			fold(ptr == (void *)&idr ? IDRP_CANON :
			     ptr == (void *)&ida ? IDAP_CANON :
			     ptr == (void *)&rt ? RRT_CANON : efold(ptr));
		} else if (*f == 'z') {
			f++;		/* %zu */
			fold(va_arg(ap, unsigned long));
		} else if (*f == '%') {
			fold(0);
		} else {
			fold(0x5ea1);
		}
	}
}

void kshim_print(const char *fmt, ...)
{
	va_list ap;
	va_start(ap, fmt);
	kprintv(fmt, ap);
	va_end(ap);
}

/* idr.c's ida_dump calls these (xarray.c only defines them under
 * XA_DEBUG): deterministic canonical folds of the printed values. */
void xa_dump_index(unsigned long index, unsigned int shift)
{
	fold(index);
	fold(shift);
}

void xa_dump_node(const struct xa_node *node)
{
	int s, m;
	if (!node) {
		fold(0);
		return;
	}
	fold(ncanon(node));
	fold(node->shift);
	fold(node->offset);
	fold(node->count);
	fold(node->nr_values);
	fold(ncanon(node->parent));
	fold(arraycanon(node->array));
	for (s = 0; s < XA_CHUNK_SIZE; s++)
		fold(efold(node->slots[s]));
	for (m = 0; m < XA_MAX_MARKS; m++)
		fold(node->marks[m][0]);
}

/* ---- generators ---- */

static unsigned long gen_idx(uint64_t r)
{
	switch (r % 12) {
	case 0: case 1: case 2:
		return (r >> 8) % 8;
	case 3: case 4:
		return (r >> 8) % 64;
	case 5:
		return 62 + (r >> 8) % 5;		/* 62..66 boundary */
	case 6:
		return (r >> 8) % 4096;
	case 7:
		return 4094 + (r >> 8) % 4;
	case 8:
		return (r >> 8) % (1UL << 20);
	case 9:
		return ((r >> 8) % (1UL << 16)) << 12;
	case 10:
		return ((r >> 8) % (1UL << 12)) << 30;
	default:
		return ULONG_MAX - (r >> 8) % 4;
	}
}

/* IDs for idr: dense small ids dominate (real trees stay shallow), with
 * boundary hits near 63/64, mid ids, int-max edges and out-of-range. */
static unsigned long gen_id(uint64_t r)
{
	switch (r % 12) {
	case 0: case 1: case 2:
		return (r >> 8) % 8;
	case 3: case 4:
		return (r >> 8) % 70;
	case 5:
		return 60 + (r >> 8) % 10;
	case 6:
		return (r >> 8) % 4096;
	case 7:
		return (r >> 8) % (1U << 20);
	case 8:
		return (r >> 8) % 200;			/* dense */
	case 9:
		return (unsigned int)INT_MAX - (r >> 8) % 4;
	case 10:
		return 0x80000000UL + (r >> 8) % 8;	/* > INT_MAX */
	default:
		return (r >> 8) % 3 ? (r >> 8) % 3000 : 0xffffffffUL;
	}
}

/* int start for idr_alloc: mostly small, sometimes negative (-EINVAL),
 * sometimes near INT_MAX. */
static int gen_start(uint64_t r)
{
	switch (r % 8) {
	case 0: case 1: case 2: case 3:
		return (int)((r >> 9) % 8);
	case 4:
		return (int)((r >> 9) % 80);
	case 5:
		return (int)(60 + (r >> 9) % 10);
	case 6:
		return -(int)(1 + (r >> 9) % 4);
	default:
		return (int)(INT_MAX - (r >> 9) % 8);
	}
}

/* int end for idr_alloc: <=0 means INT_MAX upstream. */
static int gen_end(uint64_t r)
{
	switch (r % 8) {
	case 0: case 1:
		return 0;
	case 2:
		return -1;
	case 3: case 4:
		return (int)((r >> 9) % 100);
	case 5:
		return (int)(60 + (r >> 9) % 10);
	case 6:
		return (int)((r >> 9) % 4096);
	default:
		return INT_MAX;
	}
}

/* user entries for idr/radix: aligned pool pointers, tagged pointers
 * (value-classified), xa_mk_value() integers and NULL.  Entries that
 * look internal (&3 == 2) are excluded: upstream callers never hand
 * the tree internal encodings. */
static void *gen_entry(uint64_t r)
{
	switch (r % 12) {
	case 0: case 1:
		return NULL;
	case 2: case 3: case 4: case 5: case 6: case 7:
		return &epool[(r >> 8) % EP];
	case 8: case 9:
		return xa_mk_value((r >> 8) % 4096);
	default:
		return xa_tag_pointer(&epool[(r >> 8) % EP],
				      (r >> 16) & 1 ? 3 : 1);
	}
}

static gfp_t gen_gfp(uint64_t r)
{
	switch (r % 4) {
	case 0: return GFP_KERNEL;
	case 1: return GFP_ATOMIC;
	case 2: return GFP_NOWAIT;
	default: return 0;
	}
}

/* ---- full-state dump ---- */

static void dump_state(void)
{
	int i, m, s, nused = 0, b = 0;
	struct xa_node *pl;

	/* roots and IDR scalars */
	fold(idr.idr_base);
	fold(idr.idr_next);
	fold(idr.idr_rt.xa_flags);
	fold(efold(idr.idr_rt.xa_head));
	fold(ida.xa.xa_flags);
	fold(efold(ida.xa.xa_head));
	fold(rt.xa_flags);
	fold(efold(rt.xa_head));

	for (i = 0; i < NP; i++)
		nused += node_used[i];
	fold(nused);
	for (i = 0; i < NP; i++) {
		struct xa_node *n = &node_pool[i];
		if (!node_used[i])
			continue;
		fold(i);
		fold(n->shift);
		fold(n->offset);
		fold(n->count);
		fold(n->nr_values);
		fold(ncanon(n->parent));
		fold(arraycanon(n->array));
		fold(list_empty(&n->private_list) ? 0 : 1);
		for (s = 0; s < XA_CHUNK_SIZE; s++)
			fold(efold(n->slots[s]));
		for (m = 0; m < XA_MAX_MARKS; m++)
			fold(n->marks[m][0]);
	}
	/* the per-cpu preload stack (nodes chained via ->parent) */
	fold(radix_tree_preloads.nr);
	for (pl = radix_tree_preloads.nodes; pl; pl = pl->parent)
		fold(ncanon(pl));
	fold(NILX);
	/* the ida_bitmap pool */
	for (i = 0; i < NB; i++)
		b += bmap_used[i];
	fold(b);
	for (i = 0; i < NB; i++) {
		if (!bmap_used[i])
			continue;
		fold(i);
		for (s = 0; s < IDA_BITMAP_LONGS; s++)
			fold(bpool[i].bitmap[s]);
	}
	fold(NILX);
}

static void reset_all(void)
{
	int i;
	for (i = 0; i < NP; i++) {
		memset(&node_pool[i], 0, sizeof node_pool[i]);
		INIT_LIST_HEAD(&node_pool[i].private_list);
		node_used[i] = 0;
	}
	memset(epool, 0, sizeof epool);
	for (i = 0; i < EP; i++)
		epool[i] = 0x1000 + i;
	memset(bpool, 0, sizeof bpool);
	memset(bmap_used, 0, sizeof bmap_used);
	radix_tree_preloads.nodes = NULL;
	radix_tree_preloads.nr = 0;
	/* zero the three roots too: recorded cases must be self-contained
	 * (dump_state folds their scalars, so a stale tree would leak the
	 * previous case's state into this case's checksum) */
	memset(&idr, 0, sizeof idr);
	memset(&ida, 0, sizeof ida);
	memset(&rt, 0, sizeof rt);
	alloc_ctr = 0;
	kshim_warn_hits = 0;
	g_h = 0x9e3779b97f4a7c15ULL;
	radix_tree_init();
}

/* ================= IDR ops ================= */

static void op_idr_alloc(uint64_t r)
{
	int start = gen_start(r);
	int end = gen_end(r >> 16);
	void *e = gen_entry(r >> 32);
	fold((uint64_t)(int64_t)idr_alloc(&idr, e, start, end,
					 gen_gfp(r >> 48)));
}

static void op_idr_alloc_u32(uint64_t r)
{
	u32 nextid = (u32)((r >> 9) % 200);
	unsigned long max;
	void *e = gen_entry(r >> 40);
	switch (r % 8) {
	case 0: case 1: max = (r >> 12) % 80; break;
	case 2: max = nextid + (r >> 12) % 16; break;
	case 3: max = INT_MAX; break;
	case 4: max = ULONG_MAX; break;
	case 5: max = nextid ? nextid - 1 : 0; break;
	default: max = (r >> 12) % 4096; break;
	}
	fold((uint64_t)(int64_t)idr_alloc_u32(&idr, e, &nextid, max,
					     gen_gfp(r >> 56)));
	fold(nextid);
}

static void op_idr_alloc_cyclic(uint64_t r)
{
	int start = gen_start(r);
	int end = gen_end(r >> 16);
	void *e = gen_entry(r >> 32);
	fold((uint64_t)(int64_t)idr_alloc_cyclic(&idr, e, start, end,
						gen_gfp(r >> 48)));
	fold(idr.idr_next);
}

static void op_idr_remove(uint64_t r)
{
	fold(efold(idr_remove(&idr, gen_id(r))));
}

static void op_idr_find(uint64_t r)
{
	fold(efold(idr_find(&idr, gen_id(r))));
}

static int each_cb_stop;
static int each_cb(int id, void *p, void *data)
{
	(void)data;
	fold((uint64_t)(int64_t)id);
	fold(efold(p));
	return each_cb_stop;
}

static void op_idr_for_each(uint64_t r)
{
	each_cb_stop = (r & 0x4000) ? 0 : 1 + (int)((r >> 16) % 4);
	fold((uint64_t)(int64_t)idr_for_each(&idr, each_cb, NULL));
}

static void op_idr_get_next(uint64_t r)
{
	int nid = gen_start(r);
	void *e;
	int k;
	for (k = 0; k < 12; k++) {
		e = idr_get_next(&idr, &nid);
		fold(efold(e));
		fold((uint64_t)(int64_t)nid);
		if (!e)
			break;
		nid++;
	}
	fold(NILX);
}

static void op_idr_get_next_ul(uint64_t r)
{
	unsigned long nid = gen_id(r);
	void *e;
	int k;
	for (k = 0; k < 12; k++) {
		e = idr_get_next_ul(&idr, &nid);
		fold(efold(e));
		fold(nid);
		if (!e)
			break;
		nid++;
	}
	fold(NILX);
}

static void op_idr_replace(uint64_t r)
{
	fold(efold(idr_replace(&idr, gen_entry(r >> 20), gen_id(r))));
}

static void op_idr_destroy(uint64_t r)
{
	idr_destroy(&idr);
	fold(efold(idr.idr_rt.xa_head));
	fold(idr.idr_rt.xa_flags);
	if (r & 0x80000)
		idr_init_base(&idr, (int)((r >> 20) % 4) * 4);
}

static void op_idr_preload(uint64_t r)
{
	void *e = gen_entry(r >> 30);
	int start = gen_start(r >> 4);
	int end = gen_end(r >> 44);
	int res;
	idr_preload(gen_gfp(r >> 10));
	res = idr_alloc(&idr, e, start, end, gen_gfp(r >> 52) & ~__GFP_DIRECT_RECLAIM);
	idr_preload_end();
	fold((uint64_t)(int64_t)res);
}

static void op_idr_misc(uint64_t r)
{
	switch (r % 4) {
	case 0:
		fold(idr_is_empty(&idr));
		break;
	case 1:
		fold(idr_get_cursor(&idr));
		break;
	case 2:
		idr_set_cursor(&idr, (unsigned int)((r >> 10) % 200));
		fold(idr.idr_next);
		break;
	default:
		idr_init_base(&idr, (int)((r >> 10) % 4) * 4);
		fold(idr.idr_base);
		break;
	}
}

uint64_t wrap_idr_seq(uint64_t seed, int64_t nops_i, int64_t seed2)
{
	int nops = (int)(iabs(nops_i) % 140);
	int dump_every = 5 + (int)(iabs(seed2) % 7);
	int i;

	reset_all();
	idr_init_base(&idr, (int)(iabs(seed2) % 4) * 4);
	rng_state = seed | 1;
	fail_mask = iabs(seed2) ? iabs(seed2) * 0x9e3779b97f4a7c15ULL : 0;
	if (iabs(seed2) % 5 == 1)
		fail_mask = ~0ULL;

	for (i = 0; i < nops; i++) {
		uint64_t r = xs64();
		if (trace_on()) fprintf(stderr, "OP %d %016llx\n", i, (unsigned long long)r);
		switch ((int)(r % 16)) {
		case 0: case 1: case 2:
			op_idr_alloc(r);
			break;
		case 3:
			op_idr_alloc_u32(r);
			break;
		case 4: case 5:
			op_idr_alloc_cyclic(r);
			break;
		case 6:
			op_idr_remove(r);
			break;
		case 7: case 8:
			op_idr_find(r);
			break;
		case 9:
			op_idr_replace(r);
			break;
		case 10:
			op_idr_for_each(r);
			break;
		case 11:
			op_idr_get_next(r);
			break;
		case 12:
			op_idr_get_next_ul(r);
			break;
		case 13:
			op_idr_destroy(r);
			break;
		case 14:
			op_idr_preload(r);
			break;
		default:
			op_idr_misc(r);
			break;
		}
		if (i % dump_every == dump_every - 1)
			dump_state();
	}
	dump_state();
	fold(kshim_warn_hits);
	return g_h;
}

/* ================= IDA ops ================= */

static void op_ida_alloc_range(uint64_t r)
{
	unsigned int min, max;
	switch (r % 8) {
	case 0: case 1:
		min = (unsigned int)((r >> 9) % 80);
		max = min + (unsigned int)((r >> 20) % 120);
		break;
	case 2:
		min = (unsigned int)((r >> 9) % 4) * 64;
		max = ~0U;
		break;
	case 3:
		min = (unsigned int)(INT_MAX - (r >> 9) % 8);
		max = ~0U;
		break;
	case 4:
		min = (unsigned int)((r >> 9) % 3000);
		max = min + (unsigned int)((r >> 20) % 64);
		break;
	case 5:
		min = (unsigned int)((r >> 9) % 130);
		max = min > 10 ? min - 1 - (unsigned int)((r >> 20) % 5) : 0;
		break;
	case 6:
		min = 0x80000000U + (unsigned int)((r >> 9) % 8);
		max = ~0U;
		break;
	default:
		min = (unsigned int)((r >> 9) % 4096);
		max = min + (unsigned int)((r >> 20) % 200);
		break;
	}
	fold((uint64_t)(int64_t)ida_alloc_range(&ida, min, max,
					      gen_gfp(r >> 40)));
}

static void op_ida_free(uint64_t r)
{
	unsigned long before = kshim_warn_hits;
	ida_free(&ida, (unsigned int)(gen_id(r) % 5000));
	fold(kshim_warn_hits != before);
}

static void op_ida_burst(uint64_t r)
{
	int n = 1 + (int)((r >> 8) % 6), k;
	for (k = 0; k < n; k++)
		fold((uint64_t)(int64_t)ida_alloc_range(&ida, 0, ~0U,
						      gen_gfp(r >> 20)));
}

static void op_ida_query(uint64_t r)
{
	switch (r % 4) {
	case 0:
		fold(ida_is_empty(&ida));
		break;
	case 1:
		fold(ida_exists(&ida, (unsigned int)(gen_id(r >> 10) % 5000)));
		break;
	case 2:
		fold((uint64_t)(int64_t)ida_find_first(&ida));
		break;
	default:
		fold((uint64_t)(int64_t)ida_find_first_range(&ida,
			(unsigned int)((r >> 10) % 100),
			(unsigned int)((r >> 10) % 100) +
			(unsigned int)((r >> 22) % 200)));
		break;
	}
}

static void op_ida_destroy(uint64_t r)
{
	(void)r;
	ida_destroy(&ida);
	fold(efold(ida.xa.xa_head));
}

static void ida_dump(struct ida *ida);	/* defined in idr.c below */

static void op_ida_dump(uint64_t r)
{
	(void)r;
	ida_dump(&ida);
}

uint64_t wrap_ida_seq(uint64_t seed, int64_t nops_i, int64_t seed2)
{
	int nops = (int)(iabs(nops_i) % 170);
	int dump_every = 6 + (int)(iabs(seed2) % 6);
	int i;

	reset_all();
	ida_init(&ida);
	rng_state = seed | 1;
	fail_mask = iabs(seed2) ? iabs(seed2) * 0x9e3779b97f4a7c15ULL : 0;
	if (iabs(seed2) % 5 == 1)
		fail_mask = ~0ULL;

	for (i = 0; i < nops; i++) {
		uint64_t r = xs64();
		if (trace_on()) fprintf(stderr, "OP %d %016llx\n", i, (unsigned long long)r);
		switch ((int)(r % 16)) {
		case 0: case 1: case 2: case 3:
			op_ida_alloc_range(r);
			break;
		case 4: case 5:
			op_ida_burst(r);
			break;
		case 6: case 7: case 8:
			op_ida_free(r);
			break;
		case 9: case 10:
			op_ida_query(r);
			break;
		case 11:
			op_ida_destroy(r);
			break;
		case 12:
			op_ida_dump(r);
			break;
		case 13:
			fold((uint64_t)(int64_t)ida_alloc(&ida,
							gen_gfp(r >> 20)));
			break;
		case 14:
			fold((uint64_t)(int64_t)ida_alloc_min(&ida,
				(unsigned int)((r >> 10) % 200),
				gen_gfp(r >> 20)));
			break;
		default:
			fold((uint64_t)(int64_t)ida_alloc_max(&ida,
				(unsigned int)((r >> 10) % 500),
				gen_gfp(r >> 20)));
			break;
		}
		if (i % dump_every == dump_every - 1)
			dump_state();
	}
	dump_state();
	fold(kshim_warn_hits);
	return g_h;
}

/* ================= radix-tree ops ================= */

static void op_rt_insert(uint64_t r)
{
	unsigned long idx = gen_idx(r);
	void *e = gen_entry(r >> 20);
	fold((uint64_t)(int64_t)radix_tree_insert(&rt, idx, e));
}

static void op_rt_delete(uint64_t r)
{
	unsigned long idx = gen_idx(r);
	void *item;
	void *ret;
	if (r & 0x20000) {
		ret = radix_tree_delete(&rt, idx);
	} else {
		/* delete_item with an often-wrong expected item */
		item = (r & 0x40000) ? gen_entry(r >> 30) : NULL;
		ret = radix_tree_delete_item(&rt, idx, item);
	}
	fold(efold(ret));
}

static void op_rt_lookup(uint64_t r)
{
	void __rcu **slot;
	if (r & 0x10000) {
		fold(efold(radix_tree_lookup(&rt, gen_idx(r))));
	} else {
		slot = radix_tree_lookup_slot(&rt, gen_idx(r));
		fold(scanon(slot));
		fold(slot ? efold(*slot) : NILX);
	}
}

static void op_rt_replace_slot(uint64_t r)
{
	void __rcu **slot = radix_tree_lookup_slot(&rt, gen_idx(r));
	fold(scanon(slot));
	if (slot)
		radix_tree_replace_slot(&rt, slot, gen_entry(r >> 20));
	fold(slot ? efold(*slot) : NILX);
}

static void op_rt_tags(uint64_t r)
{
	unsigned long idx = gen_idx(r);
	unsigned int tag = (unsigned int)((r >> 12) % RADIX_TREE_MAX_TAGS);
	switch (r >> 16 & 3) {
	case 0:
		/* contract: tagging a not-present item is a BUG — only set
		 * on live indices (upstream callers guarantee presence) */
		if (radix_tree_lookup(&rt, idx))
			fold(efold(radix_tree_tag_set(&rt, idx, tag)));
		else
			fold(NILX);
		break;
	case 1:
		fold(efold(radix_tree_tag_clear(&rt, idx, tag)));
		break;
	case 2:
		fold((uint64_t)radix_tree_tag_get(&rt, idx, tag));
		break;
	default:
		fold((uint64_t)radix_tree_tagged(&rt, tag));
		break;
	}
}

static void op_rt_gang(uint64_t r)
{
	void *results[14];
	unsigned long first = gen_idx(r) % 4096;
	unsigned int max = 1 + (unsigned int)((r >> 20) % 14);
	unsigned int tag = (unsigned int)((r >> 28) % RADIX_TREE_MAX_TAGS);
	unsigned int got, i;
	switch (r >> 30 & 3) {
	case 0:
		got = radix_tree_gang_lookup(&rt, results, first, max);
		break;
	case 1:
		got = radix_tree_gang_lookup_tag(&rt, results, first, max,
					       tag);
		break;
	case 2: {
		void __rcu **slots[14];
		got = radix_tree_gang_lookup_tag_slot(&rt, slots, first,
						      max, tag);
		fold(got);
		for (i = 0; i < got; i++)
			fold(scanon(slots[i]));
		return;
	}
	default:
		got = radix_tree_gang_lookup(&rt, results, first, max);
		break;
	}
	fold(got);
	for (i = 0; i < got; i++)
		fold(efold(results[i]));
}

/* full for_each_slot walk (untagged) */
static void op_rt_walk(uint64_t r)
{
	struct radix_tree_iter iter;
	void __rcu **slot;
	unsigned long start = gen_idx(r) % 4096;
	int n = 0;
	memset(&iter, 0, sizeof iter);
	radix_tree_for_each_slot(slot, &rt, &iter, start) {
		fold(iter.index);
		fold(efold(rcu_dereference_raw(*slot)));
		if (++n > 90)
			break;
	}
	fold(n);
	fold(NILX);
}

/* tagged walk */
static void op_rt_walk_tagged(uint64_t r)
{
	struct radix_tree_iter iter;
	void __rcu **slot;
	unsigned long start = gen_idx(r) % 4096;
	unsigned int tag = (unsigned int)((r >> 20) % RADIX_TREE_MAX_TAGS);
	int n = 0;
	memset(&iter, 0, sizeof iter);
	radix_tree_for_each_tagged(slot, &rt, &iter, start, tag) {
		fold(iter.index);
		fold(iter.tags);
		fold(efold(rcu_dereference_raw(*slot)));
		if (++n > 90)
			break;
	}
	fold(n);
	fold(NILX);
}

/* contiguous walk: single chunk + next_slot, then continue chunks */
static void op_rt_walk_contig(uint64_t r)
{
	struct radix_tree_iter iter;
	void __rcu **slot;
	unsigned long start = gen_idx(r) % 4096;
	int n = 0;
	memset(&iter, 0, sizeof iter);
	slot = radix_tree_iter_lookup(&rt, &iter, start);
	while (slot) {
		fold(iter.index);
		fold(iter.next_index);
		fold(radix_tree_chunk_size(&iter));
		fold(efold(rcu_dereference_raw(*slot)));
		slot = radix_tree_next_slot(slot, &iter,
					    RADIX_TREE_ITER_CONTIG);
		if (!slot && !(r & 0x80000))
			slot = radix_tree_next_chunk(&rt, &iter,
						     RADIX_TREE_ITER_CONTIG);
		if (++n > 90)
			break;
	}
	fold(n);
	fold(NILX);
}

/* mutating walk: replace or delete the k-th visited slot via the
 * iterator API (iter->node/iter->index drive the internal paths) */
static void op_rt_walk_mutate(uint64_t r)
{
	struct radix_tree_iter iter;
	void __rcu **slot;
	unsigned long start = gen_idx(r) % 4096;
	unsigned int tag = (unsigned int)((r >> 24) % RADIX_TREE_MAX_TAGS);
	unsigned int which = (unsigned int)((r >> 28) % 3);
	unsigned int pick = (unsigned int)((r >> 20) % 6);
	int n = 0;
	void *newe = gen_entry(r >> 34);
	memset(&iter, 0, sizeof iter);
	radix_tree_for_each_slot(slot, &rt, &iter, start) {
		fold(iter.index);
		fold(efold(rcu_dereference_raw(*slot)));
		if ((unsigned)n == pick) {
			switch (which) {
			case 0:
				radix_tree_iter_replace(&rt, &iter, slot, newe);
				fold(efold(rcu_dereference_raw(*slot)));
				break;
			case 1:
				radix_tree_iter_delete(&rt, &iter, slot);
				fold(iter.index);
				break;
			default:
				radix_tree_iter_tag_clear(&rt, &iter, tag);
				fold(tag);
				break;
			}
		}
		if (++n > 50)
			break;
	}
	fold(n);
	fold(NILX);
}

/* resume-in-the-middle walk: exercises radix_tree_iter_resume */
static void op_rt_walk_resume(uint64_t r)
{
	struct radix_tree_iter iter;
	void __rcu **slot;
	unsigned long start = gen_idx(r) % 4096;
	unsigned int pick = (unsigned int)((r >> 20) % 4);
	int n = 0;
	memset(&iter, 0, sizeof iter);
	slot = radix_tree_iter_init(&iter, start);
	for (;;) {
		if (!slot)
			slot = radix_tree_next_chunk(&rt, &iter, 0);
		if (!slot)
			break;
		for (;;) {
			fold(iter.index);
			fold(efold(rcu_dereference_raw(*slot)));
			if ((unsigned)n == pick)
				slot = radix_tree_iter_resume(slot, &iter);
			else
				slot = radix_tree_next_slot(slot, &iter, 0);
			if (++n > 90)
				break;
			if (!slot)
				break;
		}
		if (n > 90)
			break;
	}
	fold(n);
	fold(NILX);
}

static void op_rt_preload(uint64_t r)
{
	int res;
	fold((uint64_t)(int64_t)radix_tree_maybe_preload(gen_gfp(r >> 10)));
	res = radix_tree_maybe_preload(gen_gfp(r >> 10));
	fold((uint64_t)(int64_t)res);
	fold((uint64_t)(int64_t)radix_tree_insert(&rt, gen_idx(r >> 20),
						 gen_entry(r >> 40)));
	radix_tree_preload_end();
	fold((uint64_t)(int64_t)radix_tree_preload(gen_gfp(r >> 30)));
	fold((uint64_t)(int64_t)radix_tree_insert(&rt, gen_idx(r >> 50),
						 gen_entry(r >> 56)));
	radix_tree_preload_end();
}

static void op_rt_drain(uint64_t r)
{
	struct radix_tree_iter iter;
	void __rcu **slot;
	int n = 0;
	memset(&iter, 0, sizeof iter);
	radix_tree_for_each_slot(slot, &rt, &iter, 0) {
		radix_tree_iter_delete(&rt, &iter, slot);
		if (++n > 200)
			break;
	}
	fold(n);
	fold(efold(rt.xa_head));
}

uint64_t wrap_radix_seq(uint64_t seed, int64_t nops_i, int64_t seed2)
{
	int nops = (int)(iabs(nops_i) % 170);
	int dump_every = 5 + (int)(iabs(seed2) % 7);
	int i;

	reset_all();
	INIT_RADIX_TREE(&rt, (iabs(seed2) & 2) ? GFP_KERNEL : 0);
	rng_state = seed | 1;
	fail_mask = iabs(seed2) ? iabs(seed2) * 0x9e3779b97f4a7c15ULL : 0;
	if (iabs(seed2) % 5 == 1)
		fail_mask = ~0ULL;

	for (i = 0; i < nops; i++) {
		uint64_t r = xs64();
		if (trace_on()) fprintf(stderr, "OP %d %016llx\n", i, (unsigned long long)r);
		switch ((int)(r % 16)) {
		case 0: case 1: case 2: case 3:
			op_rt_insert(r);
			break;
		case 4: case 5:
			op_rt_delete(r);
			break;
		case 6: case 7:
			op_rt_lookup(r);
			break;
		case 8:
			op_rt_tags(r);
			break;
		case 9:
			op_rt_gang(r);
			break;
		case 10:
			op_rt_walk(r);
			break;
		case 11:
			op_rt_walk_tagged(r);
			break;
		case 12:
			op_rt_walk_contig(r);
			break;
		case 13:
			op_rt_walk_mutate(r);
			break;
		case 14:
			op_rt_walk_resume(r);
			break;
		default:
			switch ((r >> 4) % 3) {
			case 0: op_rt_preload(r); break;
			case 1: op_rt_drain(r); break;
			default: op_rt_replace_slot(r); break;
			}
			break;
		}
		if (i % dump_every == dump_every - 1)
			dump_state();
	}
	dump_state();
	fold(kshim_warn_hits);
	return g_h;
}

/* idr.c is textually included so the wrappers can drive its static
 * helpers (ida_dump).  It stays a byte-identical upstream copy;
 * check-sources.sh verifies that.  The prototypes below cover the
 * externs its #ifndef __KERNEL__ section expects.  It lives under
 * upstream/ so `rust-anything record` (which compiles every *.c at the
 * package root) does not link it a second time as its own object. */
#include "upstream/idr.inc"
