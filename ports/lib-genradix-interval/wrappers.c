/* Scalar wrappers around lib/generic-radix-tree.c (+ the
 * include/linux/generic-radix-tree.h inline layer) and
 * lib/interval_tree.c (+ include/linux/interval_tree_generic.h via the
 * concrete INTERVAL_TREE_DEFINE instantiation inside interval_tree.c),
 * so the standard scalar bridge can drive them.
 *
 * genradix: two concrete instantiations are exercised --
 *   GENRADIX(uint64_t)   power-of-2 object size, 64 objects per page
 *   GENRADIX(struct gobj)  sizeof == 24, non power-of-2: 21 objects per
 *                        page with an 8-byte remainder, which exercises
 *                        __idx_to_offset()'s divide path and the
 *                        iterator page-boundary/remainder logic.
 * Index spaces vary per variant: dense (< ~150 objects, single-leaf
 * contention), medium (straddles depth boundaries) and sparse (deep,
 * mostly-empty trees). Node allocation can be made to fail after a
 * deterministic number of calls via genradix_alloc_budget so the ENOMEM
 * paths in __genradix_ptr_alloc are covered too.
 *
 * interval_tree: a pool of ITN nodes wraps struct interval_tree_node;
 * node identity is the pool index (0..ITN-1), NULL folds as NILX. The
 * CONFIG_INTERVAL_TREE_SPAN_ITER build option is enabled in the harness
 * shim so the span iterator is exercised as well.
 *
 * Every wrapper returns one order-sensitive FNV-1a-style checksum:
 *   h = (h ^ v) * 0x100000001b3
 * folded over each op's results (allocated/looked-up values, iterator
 * positions, query node indices, span bounds) and finally over a full
 * dump of the container's observable state. Raw pointers are never
 * folded, so the checksum is address-independent.
 */
#include <stdint.h>
#include <stddef.h>
#include <string.h>
#include <linux/types.h>
#include <linux/generic-radix-tree.h>
#include <linux/rbtree.h>
#include <linux/interval_tree.h>
#include <linux/gfp.h>

int genradix_alloc_budget = -1;

#define N 64
#define ITN 64
#define NILX 0xffffu
#define ITKEY (4u * ITN)
#define ITSPAN 96u

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
static void fold(uint64_t v)
{
	g_h = (g_h ^ v) * 0x100000001b3ULL;
}

/* ================= generic radix tree ================= */

struct gobj {
	uint64_t a;
	uint64_t b;
	uint32_t c;
}; /* sizeof == 24: non power-of-2, page remainder 8 */

static GENRADIX(uint64_t) g64;
static GENRADIX(struct gobj) gnz;

static void genradix_walk_fwd_u64(uint64_t start, int cap)
{
	struct genradix_iter it = genradix_iter_init(&g64, start);
	uint64_t *p;
	int i;

	fold(it.offset);
	for (i = 0; i < cap; i++) {
		p = genradix_iter_peek(&it, &g64);
		if (!p)
			break;
		fold(it.pos);
		fold(*p);
		genradix_iter_advance(&it, &g64);
	}
	fold(p ? 1 : 0);
	fold(i);
}

static void genradix_walk_rev_u64(int cap)
{
	struct genradix_iter it =
		genradix_iter_init(&g64, genradix_last_pos(&g64));
	uint64_t *p;
	int i;

	for (i = 0; i < cap; i++) {
		p = genradix_iter_peek_prev(&it, &g64);
		if (!p)
			break;
		fold(it.pos);
		fold(*p);
		genradix_iter_rewind(&it, &g64);
	}
	fold(p ? 1 : 0);
	fold(i);
}

static void genradix_walk_fwd_gobj(uint64_t start, int cap)
{
	struct genradix_iter it = genradix_iter_init(&gnz, start);
	struct gobj *p;
	int i;

	fold(it.offset);
	for (i = 0; i < cap; i++) {
		p = genradix_iter_peek(&it, &gnz);
		if (!p)
			break;
		fold(it.pos);
		fold(p->a);
		fold(p->b);
		fold(p->c);
		genradix_iter_advance(&it, &gnz);
	}
	fold(p ? 1 : 0);
	fold(i);
}

static void genradix_walk_rev_gobj(int cap)
{
	struct genradix_iter it =
		genradix_iter_init(&gnz, genradix_last_pos(&gnz));
	struct gobj *p;
	int i;

	for (i = 0; i < cap; i++) {
		p = genradix_iter_peek_prev(&it, &gnz);
		if (!p)
			break;
		fold(it.pos);
		fold(p->a);
		fold(p->b);
		fold(p->c);
		genradix_iter_rewind(&it, &gnz);
	}
	fold(p ? 1 : 0);
	fold(i);
}

/* One mixed op sequence on one of the two genradixes.
 * variant: 0..2 u64 tree, 3..5 gobj tree; index space widens with
 * variant&3 (0 dense, 1 medium, 2 sparse). */
static void genradix_ops(int nops, int variant)
{
	uint64_t idxspace = (variant % 3 == 0) ? 150 :
			    (variant % 3 == 1) ? 5000 : (1ULL << 17);
	int use_gobj = variant >= 3;
	int i;

	for (i = 0; i < nops; i++) {
		uint64_t r = xs64();
		int t = (int)(r % 100);
		uint64_t idx = xs64() % idxspace;

		if (t < 45) {
			/* allocate + write */
			if (use_gobj) {
				struct gobj *p =
					genradix_ptr_alloc(&gnz, idx, GFP_KERNEL);
				fold(p ? 1 : 0);
				if (p) {
					p->a = idx;
					p->b = idx ^ 0xabcdefULL;
					p->c = (uint32_t)i;
				}
			} else {
				uint64_t *p =
					genradix_ptr_alloc(&g64, idx, GFP_KERNEL);
				fold(p ? 1 : 0);
				if (p)
					*p = idx ^ (rng_state & 0xffffffffULL);
			}
		} else if (t < 65) {
			/* lookup */
			if (use_gobj) {
				struct gobj *p = genradix_ptr(&gnz, idx);
				fold(p ? 1 : 0);
				if (p) {
					fold(p->a);
					fold(p->b);
					fold(p->c);
				}
			} else {
				uint64_t *p = genradix_ptr(&g64, idx);
				fold(p ? 1 : 0);
				if (p)
					fold(*p);
			}
		} else if (t < 75) {
			/* forward iterate from idx */
			if (use_gobj)
				genradix_walk_fwd_gobj(idx, 48);
			else
				genradix_walk_fwd_u64(idx, 48);
		} else if (t < 80) {
			/* reverse iterate whole tree */
			if (use_gobj)
				genradix_walk_rev_gobj(48);
			else
				genradix_walk_rev_u64(48);
		} else if (t < 85) {
			/* prealloc */
			uint64_t nr = xs64() % 48;
			if (use_gobj)
				fold((uint64_t)(int64_t)genradix_prealloc(&gnz,
					nr, GFP_KERNEL));
			else
				fold((uint64_t)(int64_t)genradix_prealloc(&g64,
					nr, GFP_KERNEL));
		} else if (t < 90) {
			/* preallocated node path */
			struct genradix_node *nn =
				genradix_alloc_node(GFP_KERNEL);
			fold(nn ? 1 : 0);
			if (use_gobj) {
				struct gobj *p =
					genradix_ptr_alloc_preallocated(&gnz,
						idx, &nn, GFP_KERNEL);
				fold(p ? 1 : 0);
				fold(nn ? 1 : 0); /* always consumed */
				if (p) {
					p->a = idx;
					p->b = idx;
					p->c = 7;
				}
			} else {
				uint64_t *p =
					genradix_ptr_alloc_preallocated(&g64,
						idx, &nn, GFP_KERNEL);
				fold(p ? 1 : 0);
				fold(nn ? 1 : 0);
				if (p)
					*p = idx;
			}
		} else if (t < 95) {
			/* inlined fast-path alloc */
			if (use_gobj) {
				struct gobj *p =
					genradix_ptr_alloc_inlined(&gnz, idx,
								   GFP_KERNEL);
				fold(p ? 1 : 0);
				if (p) {
					p->a = idx;
					p->b = ~idx;
					p->c = 3;
				}
			} else {
				uint64_t *p =
					genradix_ptr_alloc_inlined(&g64, idx,
								   GFP_KERNEL);
				fold(p ? 1 : 0);
				if (p)
					*p = idx + 1;
			}
		} else {
			/* free + reinit mid-run */
			if (use_gobj) {
				genradix_free(&gnz);
				genradix_init(&gnz);
			} else {
				genradix_free(&g64);
				genradix_init(&g64);
			}
			fold(0xdead);
		}
	}
}

/* Full observable dump: forward then reverse traversal, allocs off. */
static void genradix_dump(int use_gobj)
{
	int saved = genradix_alloc_budget;

	genradix_alloc_budget = -1; /* traversals never allocate anyway */
	if (use_gobj) {
		genradix_walk_fwd_gobj(0, 400);
		genradix_walk_rev_gobj(400);
	} else {
		genradix_walk_fwd_u64(0, 400);
		genradix_walk_rev_u64(400);
	}
	genradix_alloc_budget = saved;
}

uint64_t wrap_genradix_run(uint64_t seed, int64_t nops_i, int64_t variant_i,
			   int64_t budget_i)
{
	int variant = (int)(iabs(variant_i) % 6);
	int nops = (int)(iabs(nops_i) % 200);
	uint64_t nb = iabs(budget_i) % 160;
	int use_gobj = variant >= 3;

	g_h = 0xcbf29ce484222325ULL;
	rng_state = seed | 1;
	genradix_init(&g64);
	genradix_init(&gnz);
	genradix_alloc_budget = (nb < 80) ? -1 : (int)(nb - 80);
	fold((uint64_t)variant);

	genradix_ops(nops, variant);
	genradix_dump(use_gobj);
	fold(rng_state);

	genradix_free(&g64);
	genradix_free(&gnz);
	genradix_alloc_budget = -1;
	return g_h;
}

/* Sequential bulk insert + traversals + free. */
uint64_t wrap_genradix_seq(uint64_t seed, int64_t n_i, int64_t variant_i)
{
	int variant = (int)(iabs(variant_i) % 6);
	int n = (int)(iabs(n_i) % 200);
	int use_gobj = variant >= 3;
	int i;

	g_h = 0xcbf29ce484222325ULL;
	rng_state = seed | 1;
	genradix_init(&g64);
	genradix_init(&gnz);
	genradix_alloc_budget = -1;
	fold((uint64_t)variant);

	for (i = 0; i < n; i++) {
		uint64_t idx = (uint64_t)i * ((xs64() % 3) + 1);
		if (use_gobj) {
			struct gobj *p =
				genradix_ptr_alloc(&gnz, idx, GFP_KERNEL);
			fold(p ? 1 : 0);
			if (p) {
				p->a = idx;
				p->b = idx * 3;
				p->c = (uint32_t)i;
			}
		} else {
			uint64_t *p =
				genradix_ptr_alloc(&g64, idx, GFP_KERNEL);
			fold(p ? 1 : 0);
			if (p)
				*p = idx + rng_state;
		}
	}
	genradix_dump(use_gobj);
	/* lookups of every slot we may have touched */
	for (i = 0; i < n && i < 64; i++) {
		if (use_gobj) {
			struct gobj *p = genradix_ptr(&gnz, (uint64_t)i);
			fold(p ? 1 : 0);
		} else {
			uint64_t *p = genradix_ptr(&g64, (uint64_t)i);
			fold(p ? 1 : 0);
		}
	}
	genradix_free(&g64);
	genradix_free(&gnz);
	return g_h;
}

/* ================= interval tree ================= */

struct itnode {
	struct interval_tree_node n;
	int present;
};

static struct itnode ipool[ITN];
static struct rb_root_cached itree = RB_ROOT_CACHED;

static struct itnode *itn_of_rb(const struct rb_node *rb)
{
	return container_of(container_of(rb, struct interval_tree_node, rb),
			    struct itnode, n);
}
static int itidx(const struct interval_tree_node *p)
{
	return p ? (int)(container_of(p, struct itnode, n) - ipool) : (int)NILX;
}
static int itrbidx(const struct rb_node *rb)
{
	return rb ? (int)(itn_of_rb(rb) - ipool) : (int)NILX;
}

static int it_pick_victim(uint64_t r)
{
	int off = (int)(r % ITN), i;

	for (i = 0; i < ITN; i++) {
		int idx = (off + i) % ITN;
		if (ipool[idx].present)
			return idx;
	}
	return -1;
}

static void it_query_first(uint64_t qs, uint64_t ql, int walk_cap)
{
	struct interval_tree_node *p =
		interval_tree_iter_first(&itree, qs, ql);
	int i = 0;

	while (p && i < walk_cap) {
		fold(itidx(p));
		fold(p->start);
		fold(p->last);
		p = interval_tree_iter_next(p, qs, ql);
		i++;
	}
	fold(p ? 1 : 0);
	fold(i);
}

static void it_span_run(uint64_t first, uint64_t last)
{
	struct interval_tree_span_iter span;

	for (interval_tree_span_iter_first(&span, &itree, first, last);
	     !interval_tree_span_iter_done(&span);
	     interval_tree_span_iter_next(&span)) {
		fold((uint64_t)span.is_hole + 2);
		if (span.is_hole) {
			fold(span.start_hole);
			fold(span.last_hole);
		} else {
			fold(span.start_used);
			fold(span.last_used);
		}
	}
	fold(NILX);
}

static void it_span_advance(uint64_t first, uint64_t last, uint64_t pos)
{
	struct interval_tree_span_iter span;
	int j;

	interval_tree_span_iter_first(&span, &itree, first, last);
	fold((uint64_t)span.is_hole + 2);
	for (j = 0; j < 3; j++) {
		interval_tree_span_iter_advance(&span, &itree,
						pos + (uint64_t)j);
		fold((uint64_t)span.is_hole + 2);
		if (span.is_hole == 1) {
			fold(span.start_hole);
			fold(span.last_hole);
		} else if (span.is_hole == 0) {
			fold(span.start_used);
			fold(span.last_used);
		}
	}
	fold(NILX);
}

/* in-order dump: index, start, last, __subtree_last per node + leftmost */
static void it_dump(void)
{
	struct rb_node *rb = itree.rb_root.rb_node;
	struct rb_node *p;
	int cnt = 0;

	fold(itrbidx(itree.rb_leftmost));
	if (!rb) {
		fold(NILX);
		return;
	}
	p = rb;
	while (p->rb_left)
		p = p->rb_left;
	for (; p; p = rb_next(p)) {
		struct itnode *t = itn_of_rb(p);
		fold((uint64_t)(t - ipool));
		fold(t->n.start);
		fold(t->n.last);
		fold(t->n.__subtree_last);
		cnt++;
	}
	fold(cnt);
}

uint64_t wrap_interval_run(uint64_t seed, int64_t nops_i, int64_t variant_i)
{
	static const int mix[4][2] = {
		{45, 25}, {60, 25}, {35, 35}, {50, 20}
	};
	const int *m = mix[iabs(variant_i) % 4];
	int nops = (int)(iabs(nops_i) % 200);
	int i;

	g_h = 0xcbf29ce484222325ULL;
	rng_state = seed | 1;
	memset(ipool, 0, sizeof ipool);
	itree = RB_ROOT_CACHED;
	fold(iabs(variant_i) % 4);

	for (i = 0; i < nops; i++) {
		uint64_t r = xs64();
		int t = (int)(r % 100);

		if (t < m[0]) {
			int idx = (int)(xs64() % ITN);
			if (!ipool[idx].present) {
				ipool[idx].n.start = xs64() % ITKEY;
				ipool[idx].n.last = ipool[idx].n.start +
					xs64() % ITSPAN;
				interval_tree_insert(&ipool[idx].n, &itree);
				ipool[idx].present = 1;
				fold(idx);
			} else
				fold(NILX);
		} else if (t < m[0] + m[1]) {
			int idx = it_pick_victim(xs64());
			if (idx >= 0) {
				interval_tree_remove(&ipool[idx].n, &itree);
				ipool[idx].present = 0;
				fold(idx);
			} else
				fold(NILX);
		} else {
			uint64_t q = xs64() % 4;
			uint64_t qs = xs64() % ITKEY;
			uint64_t ql = qs + xs64() % (2 * ITSPAN);
			switch (q) {
			case 0:
				it_query_first(qs, ql, 64);
				break;
			case 1: {
				struct interval_tree_node *p =
					interval_tree_iter_first(&itree, qs, ql);
				fold(itidx(p));
				if (p)
					fold(itidx(interval_tree_iter_next(
							p, qs, ql)));
				break;
			}
			case 2:
				it_span_run(qs, ql);
				break;
			default:
				it_span_advance(qs, ql, xs64() % ITKEY);
				break;
			}
		}
	}
	it_dump();
	fold(rng_state);
	return g_h;
}

/* Sequential: n inserts, a query sweep, then erase all. */
uint64_t wrap_interval_seq(uint64_t seed, int64_t n_i, int64_t variant_i)
{
	int n = (int)(iabs(n_i) % (ITN + 1));
	int step = (int)(iabs(variant_i) % 5) + 1;
	int i;

	g_h = 0xcbf29ce484222325ULL;
	rng_state = seed | 1;
	memset(ipool, 0, sizeof ipool);
	itree = RB_ROOT_CACHED;
	fold((uint64_t)step);

	for (i = 0; i < n; i++) {
		ipool[i].n.start = ((uint64_t)i * step + xs64() % 3) % ITKEY;
		ipool[i].n.last = ipool[i].n.start + xs64() % ITSPAN;
		interval_tree_insert(&ipool[i].n, &itree);
		ipool[i].present = 1;
		fold(i);
	}
	it_dump();
	for (i = 0; i < 8; i++) {
		uint64_t qs = xs64() % ITKEY;
		it_query_first(qs, qs + xs64() % (2 * ITSPAN), 64);
	}
	it_span_run(0, ITKEY - 1);
	for (i = 0; i < n; i += step) {
		if (ipool[i].present) {
			interval_tree_remove(&ipool[i].n, &itree);
			ipool[i].present = 0;
			fold(i);
		}
	}
	it_dump();
	fold(rng_state);
	return g_h;
}
