/* Scalar wrappers around lib/xarray.c and the include/linux/xarray.h
 * inline layer, so the standard scalar bridge can drive them.
 *
 * Address-space model: the XArray stores real C pointers, so pointer
 * identity must be foldable.  struct xa_node lives in a static pool of
 * NP objects reached through the kmem_cache shim (the unit boundary is
 * lib/radix-tree.c's radix_tree_node_cachep; see linux/slab.h).
 * User entries are pointers into an aligned u64 entry pool plus
 * xa_mk_value() integers, xa_tag_pointer()s and XA_ZERO_ENTRY.
 *
 * efold() maps every entry/node to ONE canonical u64 (also the exact
 * encoding the Rust port uses as its pointer values):
 *   NULL                    -> 0
 *   entry pool &epool[i]    -> EBASE + i*16 (+tag for tagged pointers)
 *   node pool &node_pool[i] -> NBASE + i*4096 (|2 in slot encodings)
 *   &xa                     -> XA_CANON
 *   private_list head addr  -> LIST_CANON
 *   internal/value entries  -> the integer itself (already canonical)
 *
 * Allocation failure injection: a per-wrapper u64 fail_mask nominates
 * which alloc calls (1..64, counted in alloc_ctr) return NULL — this
 * drives xas_alloc -ENOMEM, the __xas_nomem/xas_nomem retry machine,
 * xas_destroy unwinding and split_alloc chains deterministically.
 *
 * Every wrapper returns one order-sensitive FNV-1a-style checksum folded
 * over each op's results (entries, errnos, marks, indices, orders) and a
 * periodic FULL TREE DUMP: per live pool node the scalars, parent/array
 * identity, private_list membership, all 64 slots and all 3 mark words,
 * plus xa_flags and the shadow list order.  Raw addresses never fold.
 */
#include <stdint.h>
#include <stddef.h>
#include <string.h>
#include <linux/types.h>
#include <linux/list.h>
#include <linux/container_of.h>
#include <linux/xarray.h>
#include <linux/slab.h>
#include <linux/errno.h>

#include "radix-tree.h"

#define NP 160		/* xa_node pool slots */
#define EP 96		/* user entry objects */
#define NILX 0xffffu	/* "no such index" fold sentinel */

/* canonical encodings shared with port.rs */
#define NBASE 0x40000ULL	/* node canonical base, stride 0x1000 */
#define EBASE 0x10000ULL	/* entry canonical base, stride 16 */
#define XA_CANON 0x800000ULL	/* &xa canonical */
#define LIST_CANON 0x900000ULL/* shadow list head canonical */

static struct xa_node node_pool[NP];
static uint8_t node_used[NP];
static uint64_t epool[EP] __attribute__((aligned(64)));
static struct xarray xa;
static LIST_HEAD(shadow_list);

static struct kmem_cache node_cache = { sizeof(struct xa_node) };
struct kmem_cache *radix_tree_node_cachep = &node_cache;

unsigned long kshim_warn_hits;

/* ---- deterministic allocator (slab boundary, see linux/slab.h) ---- */

static unsigned long alloc_ctr;
static uint64_t fail_mask;

void *kmem_cache_alloc_lru(struct kmem_cache *cachep, struct list_lru *lru,
			   gfp_t gfp)
{
	int i;
	(void)cachep; (void)lru; (void)gfp;
	alloc_ctr++;
	if (alloc_ctr <= 64 && (fail_mask >> (alloc_ctr - 1)) & 1)
		return NULL;
	for (i = 0; i < NP; i++)
		if (!node_used[i]) {
			node_used[i] = 1;
			return &node_pool[i];
		}
	return NULL;
}

void kmem_cache_free(struct kmem_cache *cachep, void *obj)
{
	struct xa_node *node = obj;
	(void)cachep;
	node_used[node - node_pool] = 0;
}

/* lib/radix-tree.c's version, with one harness repair documented below:
 * only zeroed nodes go back to the slab, so it clears slots+tags and
 * re-inits the private list (stale scalars remain, exactly like
 * upstream).
 *
 * Upstream contract: nodes are list_del()'d off private_list (via the
 * xa_update callback) before they reach RCU free.  Harness paths
 * without an update callback — xa_destroy(), plain xa_erase() — can
 * free a still-linked node, and bare INIT_LIST_HEAD() on a linked head
 * would leave the neighbours pointing at a self-looped freed node:
 * a cycle that hangs every later list_for_each.  list_del_init()
 * BEFORE the INIT preserves the observable post-state upstream
 * guarantees (node unlinked at free); the Rust port mirrors
 * "free -> unlink" at this same boundary. */
void radix_tree_node_rcu_free(struct rcu_head *head)
{
	struct xa_node *node = container_of(head, struct xa_node, rcu_head);

	memset(node->slots, 0, sizeof(node->slots));
	memset(node->marks, 0, sizeof(node->marks));
	if (!list_empty(&node->private_list))
		list_del_init(&node->private_list);
	INIT_LIST_HEAD(&node->private_list);
	kmem_cache_free(radix_tree_node_cachep, node);
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
static void fold(uint64_t v)
{
	g_h = (g_h ^ v) * 0x100000001b3ULL;
}

/* ---- canonicalization ---- */

static int node_idx(const struct xa_node *n)
{
	return (int)(n - node_pool);
}

static uint64_t ncanon(const struct xa_node *n)
{
	return n ? NBASE + (uint64_t)node_idx(n) * 4096 : 0;
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
		 * subtract node_pool from a fake pointer — the result is
		 * ASLR garbage, not a canonical address.  Only real pool
		 * nodes canonicalise; error sentinels fold raw. */
		if ((uintptr_t)n - (uintptr_t)node_pool <
		    sizeof(node_pool))
			return ncanon(n) | 2;
		return p;
	}
	if (p >= (uintptr_t)epool &&
	    p < (uintptr_t)epool + sizeof(epool))
		/* preserve the low tag bits: the kernel classifies entries
		 * on e & 3, so tagged epool pointers keep their tag in the
		 * canonical encoding (odd => value, like upstream). */
		return EBASE + ((p - (uintptr_t)epool) & ~7ULL) * 2 +
			(p & 3);
	return p;	/* internal/value/error entries are canonical */
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

static void *gen_entry(uint64_t r)
{
	switch (r % 12) {
	case 0: case 1:
		return NULL;
	case 2: case 3: case 4: case 5: case 6: case 7:
		return &epool[(r >> 8) % EP];
	case 8: case 9:
		return xa_mk_value((r >> 8) % 4096);
	case 10:
		return xa_tag_pointer(&epool[(r >> 8) % EP],
				      (r >> 16) & 1 ? 3 : 1);
	default:
		return XA_ZERO_ENTRY;
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

static xa_mark_t gen_mark(uint64_t r)
{
	switch (r % 4) {
	case 0: return XA_MARK_0;
	case 1: return XA_MARK_1;
	case 2: return XA_MARK_2;
	default: return XA_PRESENT;
	}
}

/* ---- full-state dump ---- */

static void dump_tree(void)
{
	int i, m, s, nused = 0;
	struct list_head *pos;

	fold(xa.xa_flags);
	fold(efold(xa.xa_head));
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
		fold(n->array == &xa ? XA_CANON :
		     n->array == (struct xarray *)1 ? 1 : 2);
		fold(list_empty(&n->private_list) ? 0 : 1);
		for (s = 0; s < XA_CHUNK_SIZE; s++)
			fold(efold(n->slots[s]));
		for (m = 0; m < XA_MAX_MARKS; m++)
			fold(n->marks[m][0]);
	}
	/* shadow list order (private_list linkage) */
	list_for_each(pos, &shadow_list)
		fold(ncanon(list_entry(pos, struct xa_node, private_list)));
	fold(NILX);
}

static void fold_state(const struct xa_state *xas)
{
	fold(xas->xa_index);
	fold(xas->xa_shift);
	fold(xas->xa_sibs);
	fold(xas->xa_offset);
	fold((uint64_t)(uintptr_t)xas->xa_node == 0 ? 0 :
	     xas->xa_node == XAS_BOUNDS ? 1 :
	     xas->xa_node == XAS_RESTART ? 3 :
	     xa_err(xas->xa_node) ? (uint64_t)(uintptr_t)xas->xa_node :
	     ncanon(xas->xa_node));
	fold(xas_error(xas) < 0 ? 0ULL - (uint64_t)xas_error(xas) : 0);
}

/* workingset-style update callback: non-empty nodes ride shadow_list */
static void upd_cb(struct xa_node *node)
{
	if (node->count && list_empty(&node->private_list))
		list_add(&node->private_list, &shadow_list);
	else if (!node->count && !list_empty(&node->private_list))
		list_del_init(&node->private_list);
}

static void reset_all(gfp_t flags)
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
	INIT_LIST_HEAD(&shadow_list);
	xa_init_flags(&xa, flags);
	alloc_ctr = 0;
	kshim_warn_hits = 0;
	g_h = 0x9e3779b97f4a7c15ULL;
}

/* ================= op helpers (shared by the drivers) ================= */

/* flags used by the drivers; matches xa_init_flags arguments */
static const gfp_t flag_tab[6] = {
	0,
	XA_FLAGS_ZERO_BUSY,
	XA_FLAGS_TRACK_FREE | XA_FLAGS_MARK(XA_MARK_0),
	XA_FLAGS_ALLOC,
	XA_FLAGS_ALLOC1,
	XA_FLAGS_ACCOUNT,
};

static void op_store(uint64_t r)
{
	unsigned long idx = gen_idx(r);
	void *e = gen_entry(r >> 7);
	void *old;
	switch (r >> 20 & 3) {
	case 0:
		old = xa_store(&xa, idx, e, gen_gfp(r >> 30));
		break;
	case 1:
		old = xa_store_bh(&xa, idx, e, gen_gfp(r >> 30));
		break;
	case 2:
		old = xa_store_irq(&xa, idx, e, gen_gfp(r >> 30));
		break;
	default:
		old = __xa_store(&xa, idx, e, gen_gfp(r >> 30));
		break;
	}
	fold(efold(old));
	fold(xa_err(old) < 0 ? 0ULL - (uint64_t)xa_err(old) : 0);
}

static void op_order_store(uint64_t r, unsigned int maxord)
{
	unsigned int order = (unsigned int)((r >> 22) % (maxord + 1));
	unsigned long idx = gen_idx(r) & ~((1UL << (order > 8 ? 8 : order)) - 1);
	void *e = gen_entry(r >> 40);
	void *old;
	XA_STATE_ORDER(xas, &xa, idx, order);

	if (e == NULL) {
		old = xas_store(&xas, NULL);
		fold(efold(old));
		fold(xas_error(&xas) < 0 ? 1 : 0);
		xas_destroy(&xas);
		return;
	}
	do {
		old = xas_store(&xas, e);
	} while (xas_nomem(&xas, gen_gfp(r >> 48)));
	fold(efold(old));
	fold(xas_error(&xas) < 0 ? 0ULL - (uint64_t)xas_error(&xas) : 0);
	fold_state(&xas);
	xas_destroy(&xas);
}

static void op_load(uint64_t r)
{
	fold(efold(xa_load(&xa, gen_idx(r))));
}

static void op_erase(uint64_t r)
{
	unsigned long idx = gen_idx(r);
	void *old;
	if (r & 0x8000)
		old = xa_erase(&xa, idx);
	else
		old = __xa_erase(&xa, idx);
	fold(efold(old));
}

static void op_cmpxchg(uint64_t r)
{
	unsigned long idx = gen_idx(r);
	void *old, *newe, *ret;
	uint64_t s = r >> 9;

	switch (s % 3) {
	case 0:
		old = xa_load(&xa, idx);	/* often the right answer */
		break;
	case 1:
		old = gen_entry(s >> 8);
		break;
	default:
		old = NULL;
		break;
	}
	newe = gen_entry(s >> 20);
	ret = xa_cmpxchg(&xa, idx, old, newe, gen_gfp(s >> 40));
	fold(efold(ret));
	fold(xa_err(ret) < 0 ? 0ULL - (uint64_t)xa_err(ret) : 0);
}

static void op_insert(uint64_t r)
{
	unsigned long idx = gen_idx(r);
	void *e = (r & 0x40000) ? NULL : gen_entry(r >> 24);
	fold((uint64_t)(int64_t)xa_insert(&xa, idx, e, gen_gfp(r >> 40)));
}

static void op_reserve(uint64_t r)
{
	unsigned long idx = gen_idx(r);
	if (r & 0x800)
		fold((uint64_t)(int64_t)xa_reserve(&xa, idx, gen_gfp(r >> 16)));
	else
		xa_release(&xa, idx);
	fold(efold(xa_load(&xa, idx)));
}

static void op_mark(uint64_t r)
{
	unsigned long idx = gen_idx(r);
	xa_mark_t m = (xa_mark_t)(unsigned int)((r >> 11) % XA_MAX_MARKS);
	switch (r >> 14 & 3) {
	case 0:
		xa_set_mark(&xa, idx, m);
		break;
	case 1:
		xa_clear_mark(&xa, idx, m);
		break;
	case 2:
		__xa_set_mark(&xa, idx, m);
		break;
	default:
		__xa_clear_mark(&xa, idx, m);
		break;
	}
	fold(xa_get_mark(&xa, idx, m));
	fold(xa_marked(&xa, m));
}

/* UB curation: xa_find() and xa_find_after() retry unbounded.  When a
 * walk lands on an internal-level sibling whose master slot now holds a
 * node child, xas_load() yields XA_RETRY_ENTRY and xas_retry() resets to
 * XAS_RESTART — the next walk hits the identical sibling and retries
 * forever (observed livelock: sibling at a shifted node pointing at a
 * node-child master).  xa_find_after() can likewise spin on its
 * xas_sibling() skip.  These helpers replicate the upstream functions
 * exactly but cap total find iterations so the fold stream stays
 * faithful and finite. */
static void *xa_find_b(struct xarray *xa, unsigned long *indexp,
			unsigned long max, xa_mark_t filter)
{
	XA_STATE(xas, xa, *indexp);
	void *entry;
	int tries = 0;

	rcu_read_lock();
	do {
		if ((__force unsigned int)filter < XA_MAX_MARKS)
			entry = xas_find_marked(&xas, max, filter);
		else
			entry = xas_find(&xas, max);
	} while (xas_retry(&xas, entry) && ++tries < 256);
	rcu_read_unlock();

	if (entry)
		*indexp = xas.xa_index;
	return entry;
}

static void *xa_find_after_b(struct xarray *xa, unsigned long *indexp,
			unsigned long max, xa_mark_t filter)
{
	XA_STATE(xas, xa, *indexp + 1);
	void *entry;
	int tries = 0;

	if (xas.xa_index == 0)
		return NULL;

	rcu_read_lock();
	for (;;) {
		if ((__force unsigned int)filter < XA_MAX_MARKS)
			entry = xas_find_marked(&xas, max, filter);
		else
			entry = xas_find(&xas, max);

		if (xas_invalid(&xas))
			break;
		/* xas_sibling() is static upstream; inline the same test */
		if (IS_ENABLED(CONFIG_XARRAY_MULTI) && xas.xa_node) {
			unsigned long mask =
				(XA_CHUNK_SIZE << xas.xa_node->shift) - 1;
			if ((xas.xa_index & mask) >
			    ((unsigned long)xas.xa_offset <<
			     xas.xa_node->shift)) {
				if (++tries >= 256)
					break;
				continue;
			}
		}
		if (!xas_retry(&xas, entry) || ++tries >= 256)
			break;
	}
	rcu_read_unlock();

	if (entry)
		*indexp = xas.xa_index;
	return entry;
}

static void op_find(uint64_t r)
{
	unsigned long idx = gen_idx(r);
	unsigned long max = (r & 0x100000) ? ULONG_MAX : gen_idx(r >> 21);
	xa_mark_t f = gen_mark(r >> 42);
	void *e = xa_find_b(&xa, &idx, max, f);
	fold(efold(e));
	fold(idx);
}

static void op_find_after(uint64_t r)
{
	unsigned long idx = gen_idx(r);
	unsigned long max = (r & 0x100000) ? ULONG_MAX : gen_idx(r >> 21);
	xa_mark_t f = gen_mark(r >> 42);
	void *e = xa_find_after_b(&xa, &idx, max, f);
	fold(efold(e));
	fold(idx);
}

static void op_extract(uint64_t r)
{
	unsigned long start = gen_idx(r);
	unsigned long max = (r & 0x200000) ? ULONG_MAX : start + (r >> 12) % 257;
	unsigned int n = 1 + (unsigned int)((r >> 30) % 8);
	xa_mark_t f = gen_mark(r >> 40);
	void *dst[8];
	unsigned int i, got = xa_extract(&xa, dst, start, max, n, f);
	fold(got);
	for (i = 0; i < got; i++)
		fold(efold(dst[i]));
}

static void op_get_order(uint64_t r)
{
	fold((uint64_t)xa_get_order(&xa, gen_idx(r)));
}

/* advanced API op: full xa_state walk, marks, pause, next/prev, reload */
static void op_xas(uint64_t r, int use_update)
{
	unsigned long idx = gen_idx(r);
	void *e;
	XA_STATE(xas, &xa, idx);

	if (use_update)
		xas_set_update(&xas, upd_cb);
	e = xas_load(&xas);
	fold(efold(e));
	fold_state(&xas);

	switch (r >> 20 & 7) {
	case 0:
		/* UB curation: xas_set_mark() on a NULL slot plants a mark
		 * bit over an empty slot — xas_find_marked() then spins
		 * forever (its NULL/sibling `continue` never advances past
		 * a marked-NULL at the current offset).  Upstream's safe
		 * API (__xa_set_mark) guards `if (entry)`; mirror that. */
		if (e)
			xas_set_mark(&xas, (xa_mark_t)((r >> 24) % XA_MAX_MARKS));
		fold(xas_get_mark(&xas, XA_MARK_0));
		fold(xas_get_mark(&xas, XA_MARK_1));
		fold(xas_get_mark(&xas, XA_MARK_2));
		break;
	case 1:
		xas_init_marks(&xas);
		fold(xas_get_mark(&xas, XA_MARK_0));
		fold(xas_get_mark(&xas, XA_MARK_1));
		fold(xas_get_mark(&xas, XA_MARK_2));
		break;
	case 2:
		xas_pause(&xas);
		fold_state(&xas);
		fold(efold(xas_load(&xas)));
		break;
	case 3: {
		int i;
		for (i = 0; i < 3; i++) {
			e = xas_next(&xas);
			fold(efold(e));
			fold_state(&xas);
		}
		break;
	}
	case 4: {
		int i;
		for (i = 0; i < 3; i++) {
			e = xas_prev(&xas);
			fold(efold(e));
			fold_state(&xas);
		}
		break;
	}
	case 5:
		xas_reset(&xas);
		fold(efold(xas_load(&xas)));
		break;
	case 6: {
		void *old = xas_store(&xas, gen_entry(r >> 30));
		while (xas_nomem(&xas, gen_gfp(r >> 50)))
			old = xas_store(&xas, gen_entry(r >> 30));
		fold(efold(old));
		break;
	}
	default:
		fold(efold(xas_store(&xas, NULL)));
		break;
	}
	xas_destroy(&xas);
}

static void op_create_range(uint64_t r)
{
	unsigned int order = (unsigned int)((r >> 22) % 5);
	unsigned long idx = gen_idx(r) & ~((1UL << (order > 6 ? 6 : order)) - 1);
	XA_STATE_ORDER(xas, &xa, idx, order);

	xas_create_range(&xas);
	fold_state(&xas);
	fold(efold(xas_store(&xas, gen_entry(r >> 40))));
	while (xas_nomem(&xas, gen_gfp(r >> 50)))
		;
	xas_destroy(&xas);
}

static void op_conflict(uint64_t r)
{
	unsigned int order = (unsigned int)((r >> 22) % 7);
	unsigned long idx = gen_idx(r) & ~((1UL << order) - 1);
	void *e;
	int cnt = 0;
	XA_STATE_ORDER(xas, &xa, idx, order);

	xas_for_each_conflict(&xas, e) {
		fold(efold(e));
		fold(xas.xa_index);
		if (++cnt > 8)
			break;
	}
	fold(cnt);
}

/* base index covered by a node: OR each ancestor's slot offset into
 * the index bits it controls (offset << parent->shift). */
static unsigned long node_base(struct xa_node *n)
{
	unsigned long idx = 0;
	for (; n->parent; n = n->parent)
		idx |= (unsigned long)n->offset << n->parent->shift;
	return idx;
}

/* xa_delete_node() is an internal helper only reachable once a node
 * has been logically detached (count -> 0 through xas_delete_node()).
 * Calling it directly on a still-linked node is a use-after-free —
 * upstream never does that — so this op drives it through the legal
 * path: pick a live leaf node and erase every canon slot until the
 * node empties and the internal delete/collapse path runs. */
static void op_delete_node(uint64_t r)
{
	int nth = (int)((r >> 9) % 8), i, vi = -1;
	struct xa_node *victim = NULL;
	unsigned long base;

	for (i = 0; i < NP; i++) {
		if (node_used[i] && node_pool[i].array == &xa &&
		    node_pool[i].parent && node_pool[i].shift == 0 &&
		    nth-- == 0) {
			victim = &node_pool[i];
			break;
		}
	}
	if (!victim)
		for (i = 0; i < NP; i++)
			if (node_used[i] && node_pool[i].array == &xa &&
			    node_pool[i].shift == 0) {
				victim = &node_pool[i];
				break;
			}
	if (!victim) {
		fold(NILX);
		return;
	}
	vi = node_idx(victim);
	base = node_base(victim);
	fold(vi);
	for (i = 0; i < XA_CHUNK_SIZE && node_used[vi]; i++) {
		void *e = victim->slots[i];
		if (e && !xa_is_sibling(e) && !xa_is_node(e))
			fold(efold(__xa_erase(&xa, base + i)));
	}
	fold(node_used[vi]);	/* 0 once the collapse freed it */
}

static void op_destroy(uint64_t r, gfp_t flags)
{
	xa_destroy(&xa);
	fold(efold(xa.xa_head));
	fold(xa.xa_flags);
	xa_init_flags(&xa, flags);
	fold(flags);
}

/* ================= wrap_xa_seq: general op mix ================= */

uint64_t wrap_xa_seq(uint64_t seed, int64_t nops_i, int64_t seed2)
{
	static const int mix[8] = {20, 15, 12, 8, 8, 10, 15, 12};
	int nops = (int)(iabs(nops_i) % 160);
	gfp_t flags = flag_tab[iabs(seed2) % 6];
	int use_update = (int)((iabs(seed2) >> 4) & 1);
	int dump_every = 5 + (int)(iabs(seed2) % 7);
	int i;

	reset_all(flags);
	rng_state = seed | 1;
	fail_mask = iabs(seed2) ? iabs(seed2) * 0x9e3779b97f4a7c15ULL : 0;
	if (iabs(seed2) % 5 == 1)
		fail_mask = ~0ULL;	/* every early alloc fails: ENOMEM storm */

	for (i = 0; i < nops; i++) {
		uint64_t r = xs64();
		int t = (int)(r % 100), c = 0;
		if (t < (c += mix[0])) {
			op_store(r);
		} else if (t < (c += mix[1])) {
			op_load(r);
		} else if (t < (c += mix[2])) {
			op_erase(r);
		} else if (t < (c += mix[3])) {
			op_cmpxchg(r);
		} else if (t < (c += mix[4])) {
			op_insert(r);
		} else if (t < (c += mix[5])) {
			switch ((r >> 4) % 4) {
			case 0: op_mark(r); break;
			case 1: op_find(r); break;
			case 2: op_find_after(r); break;
			default: op_reserve(r); break;
			}
		} else if (t < (c += mix[6])) {
			switch ((r >> 4) % 6) {
			case 0: op_xas(r, use_update); break;
			case 1: op_extract(r); break;
			case 2: op_get_order(r); break;
			case 3: op_create_range(r); break;
			case 4: op_order_store(r, 4); break;
			default: op_delete_node(r); break;
			}
		} else {
			switch ((r >> 4) % 6) {
			case 0: op_order_store(r, 7); break;
			case 1: op_conflict(r); break;
			case 2: op_destroy(r, flags); break;
			case 3: {
				/* __xa_alloc on whatever flags we run with:
				 * -EINVAL on non-ALLOC arrays is coverage */
				u32 id = 0xdead;
				struct xa_limit lim =
					XA_LIMIT((u32)((r >> 8) % 16),
						 (u32)(32 + (r >> 16) % 96));
				fold((uint64_t)(int64_t)__xa_alloc(&xa, &id,
						gen_entry(r >> 40), lim,
						gen_gfp(r >> 50)));
				fold(id);
				break;
			}
			case 4: {
				XA_STATE(xas, &xa, gen_idx(r >> 9));
				void *e = xas_find(&xas,
					(r & 0x4000) ? ULONG_MAX : gen_idx(r >> 30));
				fold(efold(e));
				fold_state(&xas);
				break;
			}
			default: {
				XA_STATE(xas, &xa, gen_idx(r >> 9));
				void *e = xas_find_marked(&xas,
					(r & 0x4000) ? ULONG_MAX : gen_idx(r >> 30),
					(xa_mark_t)((r >> 44) % XA_MAX_MARKS));
				fold(efold(e));
				fold_state(&xas);
				break;
			}
			}
		}
		if (i % dump_every == dump_every - 1)
			dump_tree();
	}
	dump_tree();
	fold(kshim_warn_hits);
	return g_h;
}

/* ================= wrap_xa_multi: multi-index / split stress ========= */

/* tracked live multi-index entries for valid split/store targets */
struct mrec {
	unsigned long base;
	unsigned char order;
	unsigned char live;
};
static struct mrec mult[24];

static void mult_note(unsigned long base, unsigned int order)
{
	int i;
	for (i = 0; i < 24; i++)
		if (!mult[i].live) {
			mult[i].base = base;
			mult[i].order = (unsigned char)order;
			mult[i].live = 1;
			return;
		}
}

static void mult_kill(unsigned long idx)
{
	int i;
	for (i = 0; i < 24; i++)
		if (mult[i].live && idx >= mult[i].base &&
		    idx - mult[i].base < (1UL << mult[i].order))
			mult[i].live = 0;
}

static int mult_pick(uint64_t r)
{
	int nth = (int)(r % 4), i;
	for (i = 0; i < 24; i++)
		if (mult[i].live && nth-- == 0)
			return i;
	for (i = 0; i < 24; i++)
		if (mult[i].live)
			return i;
	return -1;
}

/* store an order-N entry via the advanced API; records it in mult[] */
static void op_multi_store(uint64_t r, unsigned int maxord)
{
	unsigned int order = (unsigned int)((r >> 22) % (maxord + 1));
	unsigned long idx = gen_idx(r) & ~((1UL << order) - 1);
	void *e = gen_entry(r >> 40), *old;
	XA_STATE_ORDER(xas, &xa, idx, order);

	mult_kill(idx);
	mult_kill(idx + ((1UL << order) - 1));
	do {
		old = xas_store(&xas, e);
	} while (xas_nomem(&xas, gen_gfp(r >> 48)));
	fold(efold(old));
	fold_state(&xas);
	if (e && !xas_error(&xas) && order)
		mult_note(idx, order);
	xas_destroy(&xas);
}

static void op_split(uint64_t r)
{
	int mi = mult_pick(r >> 9);
	unsigned int order, neword;
	unsigned long idx;
	void *e;
	XA_STATE(xas, &xa, 0);

	if (mi < 0) {
		fold(NILX);
		return;
	}
	idx = mult[mi].base;
	order = xa_get_order(&xa, idx);
	if (!order) {
		mult[mi].live = 0;
		fold(NILX);
		return;
	}
	neword = (unsigned int)((r >> 20) % order);	/* strictly smaller */
	e = gen_entry(r >> 30);
	if (e == NULL || xa_is_advanced(e))
		e = &epool[(r >> 40) % EP];
	xas_set_order(&xas, idx + ((r >> 44) % (1U << order)), neword);
	/* UB curation: xas_split() derives the old entry's sibling span
	 * as 2^(order%6)-1, which is only correct when the canon lives
	 * at the canonical level (node->shift == order - order%6).  A
	 * leaf-level order-6 multi (canon + 63 sibs at shift 0) makes it
	 * compute canon = offset - xa_sibs < 0 and write out of bounds —
	 * upstream callers guarantee canonical placement; verify it. */
	(void)xas_load(&xas);
	if (xas.xa_node > XAS_RESTART &&
	    xas.xa_node->shift == order - (order % XA_CHUNK_SHIFT)) {
		xas_split_alloc(&xas, e, order, gen_gfp(r >> 50));
		fold(xas_error(&xas) < 0 ? 0ULL - (uint64_t)xas_error(&xas) : 0);
		if (!xas_error(&xas))
			xas_split(&xas, e, order);
	} else {
		fold(NILX);
	}
	fold_state(&xas);
	mult[mi].live = 0;
	if (neword)
		mult_note(xas.xa_index, neword);
	xas_destroy(&xas);
}

static void op_try_split(uint64_t r)
{
	int mi = mult_pick(r >> 9);
	unsigned int order, neword;
	unsigned long idx;
	void *e;
	XA_STATE(xas, &xa, 0);

	if (mi < 0) {
		fold(NILX);
		return;
	}
	idx = mult[mi].base;
	order = xa_get_order(&xa, idx);
	if (!order) {
		mult[mi].live = 0;
		fold(NILX);
		return;
	}
	/* the folio-split idiom: next order down, or the minimal one */
	neword = (r >> 20 & 1) ? order - 1 : xas_try_split_min_order(order);
	e = gen_entry(r >> 30);
	if (e == NULL || xa_is_advanced(e))
		e = &epool[(r >> 40) % EP];
	xas_set_order(&xas, idx + ((r >> 44) % (1U << order)), neword);
	/* Same canonical-placement curation as op_split: try_split uses
	 * the identical 2^(order%6)-1 sibling-span formula. */
	(void)xas_load(&xas);
	if (xas.xa_node > XAS_RESTART &&
	    xas.xa_node->shift == order - (order % XA_CHUNK_SHIFT)) {
		xas_try_split(&xas, e, order);
		fold(xas_error(&xas) < 0 ? 0ULL - (uint64_t)xas_error(&xas) : 0);
	} else {
		fold(NILX);
	}
	fold_state(&xas);
	mult[mi].live = 0;
	if (neword)
		mult_note(xas.xa_index, neword);
	xas_destroy(&xas);
}

static void op_store_range(uint64_t r)
{
	unsigned long first = gen_idx(r) % 8192;
	unsigned long last = first + (unsigned long)((r >> 21) % 300);
	void *e;
	if (r & 0x800000) {
		unsigned long t = first;
		first = last; last = t;		/* last < first: -EINVAL */
	}
	e = gen_entry(r >> 34);
	fold(efold(xa_store_range(&xa, first, last, e, gen_gfp(r >> 48))));
	mult_kill(first);
	mult_kill(last);
}

uint64_t wrap_xa_multi(uint64_t seed, int64_t nops_i, int64_t seed2)
{
	int nops = (int)(iabs(nops_i) % 140);
	gfp_t flags = flag_tab[iabs(seed2) % 6] & ~XA_FLAGS_TRACK_FREE;
	int dump_every = 4 + (int)(iabs(seed2) % 6);
	int i;

	reset_all(flags);
	rng_state = seed | 1;
	memset(mult, 0, sizeof mult);
	fail_mask = iabs(seed2) ? iabs(seed2) * 0x9e3779b97f4a7c15ULL : 0;
	if (iabs(seed2) % 5 == 1)
		fail_mask = ~0ULL;

	for (i = 0; i < nops; i++) {
		uint64_t r = xs64();
		switch ((int)(r % 16)) {
		case 0: case 1: case 2: case 3:
			op_multi_store(r, 6);
			break;
		case 4:
			op_multi_store(r, 10);
			break;
		case 5: case 6:
			op_split(r);
			break;
		case 7:
			op_try_split(r);
			break;
		case 8:
			op_store_range(r);
			break;
		case 9:
			op_load(r);
			break;
		case 10:
			op_erase(r);
			mult_kill(gen_idx(r));
			break;
		case 11:
			op_get_order(r);
			break;
		case 12:
			op_conflict(r);
			break;
		case 13:
			op_create_range(r);
			break;
		case 14:
			op_find(r);
			break;
		default:
			op_xas(r, 0);
			break;
		}
		if (i % dump_every == dump_every - 1)
			dump_tree();
	}
	dump_tree();
	fold(kshim_warn_hits);
	return g_h;
}

/* ================= wrap_xa_alloc: ID allocation ===================== */

uint64_t wrap_xa_alloc(uint64_t seed, int64_t nops_i, int64_t seed2)
{
	int nops = (int)(iabs(nops_i) % 200);
	gfp_t flags = (iabs(seed2) & 2) ? XA_FLAGS_ALLOC1 : XA_FLAGS_ALLOC;
	u32 next = (u32)(iabs(seed2) % 8);
	int dump_every = 6 + (int)(iabs(seed2) % 6);
	int i;

	reset_all(flags);
	rng_state = seed | 1;
	fail_mask = iabs(seed2) ? iabs(seed2) * 0x9e3779b97f4a7c15ULL : 0;
	if (iabs(seed2) % 5 == 1)
		fail_mask = ~0ULL;

	for (i = 0; i < nops; i++) {
		uint64_t r = xs64();
		u32 lo = (u32)((r >> 7) % 24);
		u32 hi = lo + (u32)((r >> 15) % 80);
		struct xa_limit lim = XA_LIMIT(lo, hi);
		u32 id = 0xbad;
		int err;

		switch ((int)(r % 12)) {
		case 0: case 1: case 2: case 3:
			err = xa_alloc(&xa, &id, gen_entry(r >> 30), lim,
				       gen_gfp(r >> 40));
			fold((uint64_t)(int64_t)err);
			fold(id);
			break;
		case 4: case 5:
			err = __xa_alloc(&xa, &id, gen_entry(r >> 30), lim,
					 gen_gfp(r >> 40));
			fold((uint64_t)(int64_t)err);
			fold(id);
			break;
		case 6: case 7:
			err = __xa_alloc_cyclic(&xa, &id, gen_entry(r >> 30),
						lim, &next, gen_gfp(r >> 40));
			fold((uint64_t)(int64_t)err);
			fold(id);
			fold(next);
			break;
		case 8:
			err = xa_alloc_cyclic(&xa, &id, gen_entry(r >> 30),
					      lim, &next, gen_gfp(r >> 40));
			fold((uint64_t)(int64_t)err);
			fold(id);
			fold(next);
			break;
		case 9:
			op_erase(r);
			break;
		case 10:
			op_load(r);
			break;
		default:
			op_find(r);
			break;
		}
		if (i % dump_every == dump_every - 1)
			dump_tree();
	}
	dump_tree();
	fold(kshim_warn_hits);
	return g_h;
}

/* ================= wrap_xa_iter: traversal ========================== */

uint64_t wrap_xa_iter(uint64_t seed, int64_t n_i, int64_t seed2)
{
	int n = (int)(iabs(n_i) % 90);
	int walk = (int)(iabs(seed2) % 4);
	int i;

	reset_all(flag_tab[iabs(seed2) % 6]);
	rng_state = seed | 1;
	fail_mask = 0;

	/* build phase: singles, multi-index entries, marks */
	for (i = 0; i < n; i++) {
		uint64_t r = xs64();
		if (r % 5 == 0) {
			unsigned int order = (unsigned int)((r >> 9) % 5);
			unsigned long idx = gen_idx(r) & ~((1UL << order) - 1);
			void *e = &epool[(r >> 20) % EP], *old;
			XA_STATE_ORDER(xas, &xa, idx, order);
			do {
				old = xas_store(&xas, e);
			} while (xas_nomem(&xas, GFP_KERNEL));
			fold(efold(old));
			xas_destroy(&xas);
		} else {
			unsigned long idx = gen_idx(r);
			void *e = gen_entry(r >> 9);
			if (!e)
				e = &epool[(r >> 20) % EP];
			fold(efold(xa_store(&xa, idx, e, GFP_KERNEL)));
		}
		if (r % 7 == 0)
			xa_set_mark(&xa, gen_idx(r >> 25),
				    (xa_mark_t)((r >> 33) % XA_MAX_MARKS));
		if (r % 11 == 0)
			xa_clear_mark(&xa, gen_idx(r >> 25),
				      (xa_mark_t)((r >> 33) % XA_MAX_MARKS));
	}
	dump_tree();

	/* walk phase */
	if (walk == 0) {
		unsigned long index;
		void *entry;
		int steps = 0;
		/* UB curation: xa_find_after() can return a canon index
		 * below the requested start when the start lands inside a
		 * marked multi-index entry — the returned index stops
		 * advancing and the iterator repeats forever (upstream
		 * quirk; callers must bound).  Cap iterations so the fold
		 * stream stays faithful but finite. */
		for (index = 0,
		     entry = xa_find_b(&xa, &index, ULONG_MAX, XA_PRESENT);
		     entry;
		     entry = xa_find_after_b(&xa, &index, ULONG_MAX,
					     XA_PRESENT)) {
			fold(index);
			fold(efold(entry));
			if (++steps > 80)
				break;
		}
		fold(NILX);
		fold(steps > 80);
		{
			unsigned long s = gen_idx(xs64());
			unsigned long l = s + xs64() % 5000;
			steps = 0;
			for (index = s,
			     entry = xa_find_b(&xa, &index, l, XA_PRESENT);
			     entry;
			     entry = xa_find_after_b(&xa, &index, l,
						     XA_PRESENT)) {
				fold(index);
				fold(efold(entry));
				if (++steps > 80)
					break;
			}
			fold(NILX);
			fold(steps > 80);
		}
		steps = 0;
		for (index = 0,
		     entry = xa_find_b(&xa, &index, ULONG_MAX,
				       (xa_mark_t)(iabs(seed2) % XA_MAX_MARKS));
		     entry;
		     entry = xa_find_after_b(&xa, &index, ULONG_MAX,
				    (xa_mark_t)(iabs(seed2) % XA_MAX_MARKS))) {
			fold(index);
			fold(efold(entry));
			if (++steps > 80)
				break;
		}
		fold(NILX);
		fold(steps > 80);
	} else if (walk == 1) {
		unsigned long index;
		void *entry;
		XA_STATE(xas, &xa, gen_idx(xs64()));
		unsigned long max = xs64() % 3 ? ULONG_MAX : gen_idx(xs64());
		int steps = 0;
		xas_for_each(&xas, entry, max) {
			fold(xas.xa_index);
			fold(efold(entry));
			if (++steps % 3 == 0)
				xas_pause(&xas);
			if (steps > 60)
				break;
		}
		fold(NILX);
		{
			XA_STATE(xm, &xa, 0);
			xa_mark_t mk = (xa_mark_t)((xs64() % 3));
			steps = 0;
			xas_for_each_marked(&xm, entry, ULONG_MAX, mk) {
				fold(xm.xa_index);
				fold(efold(entry));
				if (++steps > 60)
					break;
			}
			fold(NILX);
		}
		(void)index;
	} else if (walk == 2) {
		/* full sequential walk incl. holes, then reverse */
		unsigned long start = gen_idx(xs64()) % 512;
		void *e;
		int k;
		XA_STATE(xas, &xa, start);
		e = xas_load(&xas);
		fold(efold(e));
		for (k = 0; k < 80; k++) {
			e = xas_next(&xas);
			fold(efold(e));
			fold(xas.xa_index);
			if (xas.xa_node == XAS_BOUNDS)
				break;
		}
		fold(NILX);
		{
			XA_STATE(xp, &xa, xas.xa_index);
			for (k = 0; k < 80; k++) {
				e = xas_prev(&xp);
				fold(efold(e));
				fold(xp.xa_index);
				if (xp.xa_node == XAS_BOUNDS)
					break;
			}
		}
		fold(NILX);
	} else {
		/* destructive: erase odds while iterating, then re-walk */
		unsigned long index = 0;
		void *entry;
		int k = 0;
		for (;;) {
			entry = xa_find_b(&xa, &index, ULONG_MAX, XA_PRESENT);
			if (!entry)
				break;
			fold(index);
			if (k % 2 == 0)
				fold(efold(__xa_erase(&xa, index)));
			/* xa_find can report a multi-index canon below the
			 * requested start; index++ then never advances past
			 * the entry's coverage — bound the walk. */
			if (++k > 200 || index == ULONG_MAX)
				break;
			index++;
		}
		fold(NILX);
		index = 0;
		for (entry = xa_find_b(&xa, &index, ULONG_MAX, XA_PRESENT);
		     entry;
		     entry = xa_find_after_b(&xa, &index, ULONG_MAX,
					     XA_PRESENT)) {
			fold(index);
			fold(efold(entry));
		}
		fold(NILX);
	}
	dump_tree();
	fold(kshim_warn_hits);
	return g_h;
}
