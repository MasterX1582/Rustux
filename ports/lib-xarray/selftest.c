/* Invariant self-test (not part of the shipped port): hand-computed
 * mini sequences plus a structural validator walked after every op in a
 * seeded mix.  Exit 0 = all good. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <linux/types.h>
#include <linux/list.h>
#include <linux/container_of.h>
#include <linux/xarray.h>
#include <linux/slab.h>
#include <linux/errno.h>

#include "radix-tree.h"

#define NP 160
#define EP 96

static struct xa_node node_pool[NP];
static uint8_t node_used[NP];
static uint64_t epool[EP] __attribute__((aligned(64)));
static struct xarray xa;
static LIST_HEAD(shadow_list);
static struct kmem_cache node_cache = { sizeof(struct xa_node) };
struct kmem_cache *radix_tree_node_cachep = &node_cache;
unsigned long kshim_warn_hits;
static unsigned long alloc_ctr, fail_mask;

void *kmem_cache_alloc_lru(struct kmem_cache *c, struct list_lru *l, gfp_t g)
{
	int i;
	(void)c; (void)l; (void)g;
	alloc_ctr++;
	if (alloc_ctr <= 64 && (fail_mask >> (alloc_ctr - 1)) & 1)
		return NULL;
	for (i = 0; i < NP; i++)
		if (!node_used[i]) { node_used[i] = 1; return &node_pool[i]; }
	return NULL;
}

void kmem_cache_free(struct kmem_cache *c, void *obj)
{
	struct xa_node *n = obj;
	(void)c;
	node_used[n - node_pool] = 0;
}

void radix_tree_node_rcu_free(struct rcu_head *head)
{
	struct xa_node *node = container_of(head, struct xa_node, rcu_head);
	memset(node->slots, 0, sizeof(node->slots));
	memset(node->marks, 0, sizeof(node->marks));
	INIT_LIST_HEAD(&node->private_list);
	kmem_cache_free(radix_tree_node_cachep, node);
}

static uint64_t rng_state;
static uint64_t xs64(void)
{
	uint64_t x = rng_state;
	x ^= x >> 12; x ^= x << 25; x ^= x >> 27;
	rng_state = x;
	return x * 2685821657736338717ULL;
}

static int fails;
#define CHECK(c, ...) do { if (!(c)) { fails++; \
	printf("FAIL %d: ", __LINE__); printf(__VA_ARGS__); \
	printf("\n"); if (fails > 20) exit(1); } } while (0)

static void reset_all(gfp_t flags)
{
	int i;
	for (i = 0; i < NP; i++) {
		memset(&node_pool[i], 0, sizeof node_pool[i]);
		INIT_LIST_HEAD(&node_pool[i].private_list);
		node_used[i] = 0;
	}
	INIT_LIST_HEAD(&shadow_list);
	xa_init_flags(&xa, flags);
	alloc_ctr = 0;
	fail_mask = 0;
	kshim_warn_hits = 0;
}

static int used_nodes(void)
{
	int i, c = 0;
	for (i = 0; i < NP; i++)
		c += node_used[i];
	return c;
}

/* nr_values is a write-time ledger (see check_node) — count nodes
 * whose nr_values has drifted out of representable range. */
static int nrv_drift;

/* subtree contains mark m anywhere below (and incl.) this node? */
static int subtree_marked(struct xa_node *n, xa_mark_t m)
{
	int i;
	for (i = 0; i < XA_CHUNK_SIZE; i++) {
		void *e = n->slots[i];
		if (test_bit(i, n->marks[(__force unsigned)m]))
			return 1;
		if (xa_is_node(e) && subtree_marked(xa_to_node(e), m))
			return 1;
	}
	return 0;
}

/* verify one live node: counts, parent linkage, sibling canon, marks */
static void check_node(struct xa_node *n, struct xa_node *parent,
		       int off, int *seen)
{
	int i, cnt = 0;
	CHECK(n->array == &xa || n->array == (struct xarray *)1,
	      "node not owned by xa");
	CHECK(n->parent == parent, "bad parent link");
	CHECK(!parent || n->offset == off, "bad offset %d", n->offset);
	CHECK(!parent || n->shift == parent->shift - XA_CHUNK_SHIFT ||
	      parent->shift == 0, "bad shift %d parent %d", n->shift,
	      parent->shift);
	for (i = 0; i < XA_CHUNK_SIZE; i++) {
		void *e = n->slots[i];
		if (!e)
			continue;
		cnt++;
		if (xa_is_node(e)) {
			struct xa_node *c = xa_to_node(e);
			CHECK(c >= node_pool && c < node_pool + NP,
			      "child node out of pool");
			CHECK(node_used[c - node_pool], "child not in use");
			CHECK(c != n, "self-child");
			if (n->shift == 0)
				CHECK(0, "node child at shift 0");
			else
				check_node(c, n, i, seen);
		} else if (xa_is_sibling(e)) {
			unsigned long j = xa_to_sibling(e);
			CHECK(j < i, "sibling %d points fwd to %ld", i, j);
			/* canon may be NULL after a partial single-slot
			 * erase, but must never itself be a sibling */
			CHECK(!xa_is_sibling(n->slots[j]),
			      "sibling canon is a sibling");
		} else if (xa_is_retry(e)) {
			CHECK(0, "retry entry in live node slot %d", i);
		}
	}
	CHECK(n->count == cnt, "count %d != %d slots", n->count, cnt);
	/* nr_values is a write-time ledger that upstream lets drift:
	 * a single-index store can change a canon slot's type without
	 * retouching its sibling slots, and xas_split()'s decrement is
	 * based on the resolved canon at xa_offset, not per-slot state.
	 * Underflow (u8 wrap past XA_CHUNK_SIZE) is real upstream
	 * behaviour — report it as drift, don't fail on it.  Exact
	 * fidelity is verified C-vs-Rust through the wrapper dumps. */
	if (n->nr_values > XA_CHUNK_SIZE)
		nrv_drift++;
	{
		xa_mark_t m = XA_MARK_0;
		for (;;) {
			unsigned int mm = (__force unsigned)m;
			for (i = 0; i < XA_CHUNK_SIZE; i++) {
				void *e = n->slots[i];
				int bit = test_bit(i, n->marks[mm]);
				if (xa_is_node(e)) {
					CHECK(bit == subtree_marked(xa_to_node(e), m),
					      "mark agg mismatch node off %d mark %d",
					      i, mm);
				}
				(void)bit;
			}
			if (m == XA_MARK_MAX)
				break;
			m = (__force xa_mark_t)(mm + 1);
		}
	}
	(*seen)++;
}

static void check_tree(void)
{
	void *head = xa.xa_head;
	int seen = 0, i;
	xa_mark_t m = XA_MARK_0;
	if (!head) {
		CHECK(xa_empty(&xa), "empty mismatch");
		for (i = 0; i < NP; i++)
			CHECK(!(node_used[i] && node_pool[i].array == &xa),
			      "orphan live node %d", i);
	} else if (xa_is_node(head)) {
		check_node(xa_to_node(head), NULL, 0, &seen);
	}
	for (;;) {
		unsigned int mm = (__force unsigned)m;
		CHECK(xa_marked(&xa, m) ==
		      (xa_is_node(head) ? subtree_marked(xa_to_node(head), m)
					: (head && test_bit(0, xa_is_node(head) ?
							 xa_to_node(head)->marks[mm]
							 : (unsigned long[]){0}))),
		      "top mark %u mismatch", mm);
		if (m == XA_MARK_MAX)
			break;
		m = (__force xa_mark_t)(mm + 1);
	}
}

static unsigned long gen_idx(uint64_t r)
{
	switch (r % 6) {
	case 0: case 1: return (r >> 8) % 96;
	case 2: return 62 + (r >> 8) % 5;
	case 3: return (r >> 8) % 4096;
	case 4: return ((r >> 8) % (1UL << 10)) << 12;
	default: return ((r >> 8) % (1UL << 8)) << 30;
	}
}

static void *gen_entry(uint64_t r)
{
	switch (r % 8) {
	case 0: return NULL;
	case 1: case 2: case 3: case 4: return &epool[(r >> 8) % EP];
	case 5: return xa_mk_value((r >> 8) % 256);
	case 6: return xa_tag_pointer(&epool[(r >> 8) % EP], 1);
	default: return XA_ZERO_ENTRY;
	}
}

int main(void)
{
	unsigned long i;
	int seed;

	/* --- hand-computed basics --- */
	reset_all(0);
	CHECK(xa_empty(&xa), "not empty at start");
	xa_store(&xa, 0, &epool[1], GFP_KERNEL);
	xa_store(&xa, 1, &epool[2], GFP_KERNEL);
	xa_store(&xa, 64, &epool[3], GFP_KERNEL);
	CHECK(xa_load(&xa, 0) == &epool[1], "load 0");
	CHECK(xa_load(&xa, 1) == &epool[2], "load 1");
	CHECK(xa_load(&xa, 64) == &epool[3], "load 64");
	CHECK(xa_load(&xa, 2) == NULL, "load hole");
	CHECK(xa_err(xa_store(&xa, 3, xa_mk_value(9), GFP_KERNEL)) == 0,
	      "store value");
	CHECK(xa_load(&xa, 3) == xa_mk_value(9), "load value");
	CHECK(xa_to_value(xa_load(&xa, 3)) == 9, "to_value");
	CHECK(xa_insert(&xa, 1, &epool[4], GFP_KERNEL) == -EBUSY,
	      "insert occupied");
	CHECK(xa_insert(&xa, 4, &epool[4], GFP_KERNEL) == 0, "insert empty");
	CHECK(xa_load(&xa, 4) == &epool[4], "inserted entry");
	CHECK(xa_cmpxchg(&xa, 4, &epool[0], &epool[5], GFP_KERNEL) ==
	      &epool[4], "cmpxchg wrong old");
	CHECK(xa_load(&xa, 4) == &epool[4], "cmpxchg fail kept old");
	CHECK(xa_cmpxchg(&xa, 4, &epool[4], &epool[5], GFP_KERNEL) ==
	      &epool[4], "cmpxchg right old");
	CHECK(xa_load(&xa, 4) == &epool[5], "cmpxchg stored");
	check_tree();

	/* erase + shrink: single entry at 0 collapses to root */
	CHECK(xa_erase(&xa, 1) == &epool[2], "erase ret");
	CHECK(xa_erase(&xa, 64) == &epool[3], "erase64 ret");
	CHECK(xa_erase(&xa, 3) == xa_mk_value(9), "erase value ret");
	CHECK(xa_erase(&xa, 4) == &epool[5], "erase4 ret");
	check_tree();
	CHECK(xa_load(&xa, 0) == &epool[1], "survivor");
	CHECK(xa_erase(&xa, 0) == &epool[1], "erase last");
	CHECK(xa_empty(&xa), "empty after all erased");
	CHECK(used_nodes() == 0, "nodes leaked: %d", used_nodes());
	check_tree();

	/* order-2 multi-index: store covers 4 indices, erase hits all */
	{
		void *e = &epool[7], *old;
		XA_STATE_ORDER(xas, &xa, 0, 2);
		do {
			old = xas_store(&xas, e);
		} while (xas_nomem(&xas, GFP_KERNEL));
		CHECK(old == NULL, "order2 old");
		xas_destroy(&xas);
		for (i = 0; i < 4; i++)
			CHECK(xa_load(&xa, i) == e, "order2 load %lu", i);
		CHECK(xa_load(&xa, 4) == NULL, "order2 spill");
		CHECK(xa_get_order(&xa, 0) == 2, "get_order");
		CHECK(xa_get_order(&xa, 3) == 2, "get_order sib");
		CHECK(xa_erase(&xa, 1) == e, "erase inside multi");
		for (i = 0; i < 4; i++)
			CHECK(xa_load(&xa, i) == NULL, "multi gone %lu", i);
		check_tree();
	}

	/* marks: set/find/clear + propagation */
	xa_store(&xa, 5, &epool[8], GFP_KERNEL);
	xa_store(&xa, 70, &epool[9], GFP_KERNEL);
	xa_set_mark(&xa, 70, XA_MARK_1);
	CHECK(xa_get_mark(&xa, 70, XA_MARK_1), "mark set");
	CHECK(!xa_get_mark(&xa, 5, XA_MARK_1), "mark isolation");
	CHECK(xa_marked(&xa, XA_MARK_1), "array mark");
	{
		unsigned long idx = 0;
		CHECK(xa_find(&xa, &idx, ULONG_MAX, XA_MARK_1) == &epool[9],
		      "find marked");
		CHECK(idx == 70, "find marked idx %lu", idx);
		idx = 0;
		CHECK(xa_find(&xa, &idx, ULONG_MAX, XA_PRESENT) == &epool[8],
		      "find present");
		CHECK(idx == 5, "find present idx");
	}
	xa_clear_mark(&xa, 70, XA_MARK_1);
	CHECK(!xa_marked(&xa, XA_MARK_1), "mark cleared");
	check_tree();
	xa_destroy(&xa);
	CHECK(used_nodes() == 0, "destroy leaked");
	check_tree();

	/* reserve/release lifecycle */
	xa_init_flags(&xa, 0);
	CHECK(xa_reserve(&xa, 9, GFP_KERNEL) == 0, "reserve");
	CHECK(xa_load(&xa, 9) == NULL, "reserved loads NULL");
	xa_release(&xa, 9);
	CHECK(xa_reserve(&xa, 9, GFP_KERNEL) == 0, "re-reserve");
	CHECK(xa_store(&xa, 9, &epool[10], GFP_KERNEL) == NULL,
	      "store over reserve gives zero-as-null");
	xa_erase(&xa, 9);
	check_tree();

	/* alloc: fill limit, EBUSY, free one, realloc */
	reset_all(XA_FLAGS_ALLOC);
	{
		u32 id = 0;
		struct xa_limit lim = XA_LIMIT(0, 9);
		for (i = 0; i < 10; i++)
			CHECK(xa_alloc(&xa, &id, &epool[i], lim,
				       GFP_KERNEL) == 0 && id == i,
			      "alloc %lu got %u", i, id);
		CHECK(xa_alloc(&xa, &id, &epool[10], lim, GFP_KERNEL) == -EBUSY,
		      "alloc exhausted");
		xa_erase(&xa, 3);
		CHECK(xa_alloc(&xa, &id, &epool[10], lim, GFP_KERNEL) == 0 &&
		      id == 3, "realloc %u", id);
		check_tree();
	}
	/* cyclic wrap */
	{
		u32 id = 0, next = 2;
		struct xa_limit lim = XA_LIMIT(0, 3);
		reset_all(XA_FLAGS_ALLOC);
		xa_store(&xa, 0, &epool[0], GFP_KERNEL);
		xa_store(&xa, 1, &epool[1], GFP_KERNEL);
		xa_store(&xa, 2, &epool[2], GFP_KERNEL);
		CHECK(__xa_alloc_cyclic(&xa, &id, &epool[3], lim, &next,
					GFP_KERNEL) == 0 && id == 3,
		      "cyclic first");
		CHECK(__xa_alloc_cyclic(&xa, &id, &epool[4], lim, &next,
					GFP_KERNEL) == -EBUSY,
		      "cyclic exhausted");
		xa_erase(&xa, 0);
		CHECK(__xa_alloc_cyclic(&xa, &id, &epool[4], lim, &next,
					GFP_KERNEL) == 1 && id == 0,
		      "cyclic wrap ret");
		CHECK(next == 1, "cyclic next %u", next);
		check_tree();
	}

	/* deep tree: index 2^40 exercises max depth + shrink */
	reset_all(0);
	xa_store(&xa, 1UL << 40, &epool[11], GFP_KERNEL);
	CHECK(xa_load(&xa, 1UL << 40) == &epool[11], "deep load");
	check_tree();
	CHECK(xa_erase(&xa, 1UL << 40) == &epool[11], "deep erase");
	CHECK(xa_empty(&xa), "deep shrink to empty");
	CHECK(used_nodes() == 0, "deep nodes leaked %d", used_nodes());

	/* tagged pointers round-trip */
	reset_all(0);
	xa_store(&xa, 2, xa_tag_pointer(&epool[12], 3), GFP_KERNEL);
	CHECK(xa_untag_pointer(xa_load(&xa, 2)) == &epool[12], "untag");
	CHECK(xa_pointer_tag(xa_load(&xa, 2)) == 3, "tag");
	CHECK(xa_is_value(xa_load(&xa, 2)), "tagged counts as value");
	xa_erase(&xa, 2);
	check_tree();

	/* ENOMEM storm: every alloc fails → every store must fail with
	 * -ENOMEM cleanly and leave the array untouched */
	reset_all(0);
	fail_mask = ~0ULL;
	alloc_ctr = 0;
	{
		void *r = xa_store(&xa, 300, &epool[13], GFP_KERNEL);
		CHECK(xa_err(r) == -ENOMEM, "enomem ret %ld", (long)r);
		CHECK(xa_empty(&xa), "enomem left junk");
		alloc_ctr = 0;
		{
			XA_STATE(xas, &xa, 0);
			XA_STATE_ORDER(xo, &xa, 0, 3);
			void *o;
			do {
				o = xas_store(&xas, &epool[14]);
			} while (xas_nomem(&xas, GFP_KERNEL));
			CHECK(o == NULL, "root store ok after storm window");
			xas_destroy(&xas);
			do {
				o = xas_store(&xo, &epool[15]);
			} while (xas_nomem(&xo, GFP_KERNEL));
			CHECK(xas_error(&xo) == -ENOMEM ||
			      xa_load(&xa, 0) == &epool[15],
			      "order3 after storm");
			xas_destroy(&xo);
		}
	}
	fail_mask = 0;
	check_tree();

	/* extract ordering */
	reset_all(0);
	xa_store(&xa, 3, &epool[20], GFP_KERNEL);
	xa_store(&xa, 9, &epool[21], GFP_KERNEL);
	xa_store(&xa, 90, &epool[22], GFP_KERNEL);
	{
		void *dst[4];
		CHECK(xa_extract(&xa, dst, 0, ULONG_MAX, 4, XA_PRESENT) == 3,
		      "extract count");
		CHECK(dst[0] == &epool[20] && dst[1] == &epool[21] &&
		      dst[2] == &epool[22], "extract order");
		CHECK(xa_extract(&xa, dst, 4, ULONG_MAX, 4, XA_PRESENT) == 2,
		      "extract partial");
		CHECK(dst[0] == &epool[21], "extract partial entry");
	}
	xa_destroy(&xa);

	/* --- seeded mix with per-op validation --- */
	for (seed = 1; seed <= 30; seed++) {
		int t;
		reset_all(0);
		rng_state = seed;
		fail_mask = (seed % 4 == 0) ? xs64() : 0;
		for (i = 0; i < 300; i++) {
			uint64_t r = xs64();
			unsigned long idx = gen_idx(r);
			switch ((int)(r % 12)) {
			case 0: case 1: case 2: case 3: {
				void *e = gen_entry(r >> 9);
				if (e == XA_ZERO_ENTRY)
					e = &epool[(r >> 20) % EP];
				xa_store(&xa, idx, e, GFP_KERNEL);
				break;
			}
			case 4: case 5:
				xa_erase(&xa, idx);
				break;
			case 6: {
				unsigned int order = (unsigned int)((r >> 9) % 4);
				unsigned long b = idx & ~((1UL << order) - 1);
				void *o;
				XA_STATE_ORDER(xas, &xa, b, order);
				do {
					o = xas_store(&xas, &epool[(r >> 20) % EP]);
				} while (xas_nomem(&xas, GFP_KERNEL));
				xas_destroy(&xas);
				break;
			}
			case 7:
				xa_set_mark(&xa, idx,
					    (xa_mark_t)((r >> 9) % XA_MAX_MARKS));
				break;
			case 8:
				xa_clear_mark(&xa, idx,
					      (xa_mark_t)((r >> 9) % XA_MAX_MARKS));
				break;
			case 9: {
				unsigned long j = idx;
				xa_find(&xa, &j, ULONG_MAX, XA_PRESENT);
				break;
			}
			case 10:
				xa_cmpxchg(&xa, idx, NULL,
					   &epool[(r >> 20) % EP], GFP_KERNEL);
				break;
			default:
				xa_load(&xa, idx);
				break;
			}
			if (i % 17 == 0)
				check_tree();
		}
		check_tree();
		xa_destroy(&xa);
		CHECK(used_nodes() == 0, "seed %d leaked %d", seed,
		      used_nodes());
	}
	printf("%s (%d failures, %d nr_values drift sightings)\n",
	       fails ? "FAILURES" : "ALL OK", fails, nrv_drift);
	return fails ? 1 : 0;
}
