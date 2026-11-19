/* Invariant self-test (not part of the shipped port): drives the same
 * op mix as wrappers.c but validates structural invariants after ops.
 *
 *  genradix: genradix_ptr() must be NULL exactly when the entry's page
 *  was never allocated; entries in allocated pages read back what was
 *  written (0 when untouched). Forward iteration must visit every
 *  allocated slot in increasing pos order; reverse iteration must visit
 *  the same set in decreasing order.
 *
 *  interval_tree: every node's __subtree_last must equal the max last
 *  over its subtree (recomputed from the rb links); iter_first/iter_next
 *  must yield exactly the present nodes intersecting the query range in
 *  in-order; span iteration must tile the query range with alternating
 *  hole/used spans, holes not intersecting any node and used spans fully
 *  covered.
 *
 * Exit 0 = all good. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <linux/types.h>
#include <linux/generic-radix-tree.h>
#include <linux/rbtree.h>
#include <linux/interval_tree.h>
#include <linux/gfp.h>

int genradix_alloc_budget = -1;

#define ITN 64
#define ITKEY (4u * ITN)
#define MAXIDX 6000
#define MAXPAGE 94 /* ceil(6016/64): whole last page of idx 5999 */
#define MAXSLOTS (MAXPAGE * 64)

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

/* ---- genradix ---- */

struct gobj { uint64_t a; uint64_t b; uint32_t c; };

static GENRADIX(uint64_t) g64;
static GENRADIX(struct gobj) gnz;

/* which u64-object pages have ever been allocated */
static char gpage[MAXPAGE];
/* value written per idx (only slots < MAXSLOTS tracked) */
static uint64_t gval[MAXSLOTS];
static char gwritten[MAXSLOTS];

static uint64_t gidx_to_page_u64(uint64_t idx) { return idx / 64; }

static void check_u64_tree(void)
{
	uint64_t idx;
	struct genradix_iter it;
	uint64_t *p;
	uint64_t prev = 0, count = 0;
	int first = 1;

	/* lookup consistency */
	for (idx = 0; idx < MAXSLOTS; idx++) {
		p = genradix_ptr(&g64, idx);
		CHECK((p != NULL) == (gpage[gidx_to_page_u64(idx)] != 0),
		      "idx %llu: ptr %s but page %s", (unsigned long long)idx,
		      p ? "non-NULL" : "NULL",
		      gpage[gidx_to_page_u64(idx)] ? "alloc" : "free");
		if (p)
			CHECK(*p == (gwritten[idx] ? gval[idx] : 0),
			      "idx %llu: value %llu want %llu",
			      (unsigned long long)idx,
			      (unsigned long long)*p,
			      (unsigned long long)(gwritten[idx] ? gval[idx] : 0));
	}

	/* forward iteration: increasing pos covering allocated slots */
	it = genradix_iter_init(&g64, 0);
	while ((p = genradix_iter_peek(&it, &g64)) != NULL) {
		CHECK(first || it.pos > prev, "fwd pos not increasing");
		prev = it.pos;
		first = 0;
		CHECK(it.pos < MAXSLOTS &&
		      gpage[gidx_to_page_u64(it.pos)],
		      "fwd visited pos %llu in unalloc'd page",
		      (unsigned long long)it.pos);
		count++;
		genradix_iter_advance(&it, &g64);
	}
	{
		uint64_t want = 0, i;
		for (i = 0; i < MAXSLOTS; i++)
			if (gpage[gidx_to_page_u64(i)])
				want++;
		CHECK(count == want, "fwd visited %llu want %llu",
		      (unsigned long long)count, (unsigned long long)want);
	}

	/* reverse iteration: decreasing pos, same count */
	it = genradix_iter_init(&g64, genradix_last_pos(&g64));
	prev = ~0ULL;
	count = 0;
	while ((p = genradix_iter_peek_prev(&it, &g64)) != NULL) {
		CHECK(it.pos < prev, "rev pos not decreasing");
		prev = it.pos;
		CHECK(it.pos < MAXSLOTS &&
		      gpage[gidx_to_page_u64(it.pos)],
		      "rev visited pos %llu in unalloc'd page",
		      (unsigned long long)it.pos);
		count++;
		genradix_iter_rewind(&it, &g64);
	}
	{
		uint64_t want = 0, i;
		for (i = 0; i < MAXSLOTS; i++)
			if (gpage[gidx_to_page_u64(i)])
				want++;
		CHECK(count == want, "rev visited %llu want %llu",
		      (unsigned long long)count, (unsigned long long)want);
	}
}

static void genradix_test(void)
{
	int seed, i;
	for (seed = 1; seed <= 12; seed++) {
		memset(gpage, 0, sizeof gpage);
		memset(gwritten, 0, sizeof gwritten);
		genradix_init(&g64);
		rng_state = seed;
		for (i = 0; i < 300; i++) {
			uint64_t r = xs64();
			uint64_t idx = xs64() % MAXIDX;
			if (r % 100 < 55) {
				uint64_t *p =
					genradix_ptr_alloc(&g64, idx, GFP_KERNEL);
				CHECK(p != NULL, "alloc failed (budget off)");
				if (p) {
					gpage[gidx_to_page_u64(idx)] = 1;
					*p = idx ^ r;
					gval[idx] = *p;
					gwritten[idx] = 1;
				}
			} else if (r % 100 < 75) {
				uint64_t *p = genradix_ptr(&g64, idx);
				CHECK((p != NULL) ==
				      (gpage[gidx_to_page_u64(idx)] != 0),
				      "lookup mismatch idx %llu",
				      (unsigned long long)idx);
			} else if (r % 100 < 85) {
				uint64_t nr = xs64() % 40, k;
				CHECK(genradix_prealloc(&g64, nr,
							GFP_KERNEL) == 0,
				      "prealloc failed");
				for (k = 0; k <= nr; k++)
					gpage[gidx_to_page_u64(k)] = 1;
			}
		}
		check_u64_tree();
		genradix_free(&g64);
		CHECK(genradix_ptr(&g64, 0) == NULL &&
		      genradix_ptr(&g64, MAXSLOTS - 1) == NULL,
		      "tree not empty after free");
	}

	/* non-pow2 gobj tree: same lookup/page invariant */
	for (seed = 1; seed <= 6; seed++) {
		char zpage[MAXPAGE];
		struct genradix_iter it;
		struct gobj *p;
		memset(zpage, 0, sizeof zpage);
		genradix_init(&gnz);
		rng_state = seed;
		for (i = 0; i < 200; i++) {
			uint64_t idx = xs64() % 300;
			struct gobj *q =
				genradix_ptr_alloc(&gnz, idx, GFP_KERNEL);
			CHECK(q != NULL, "gobj alloc failed");
			if (q) {
				zpage[idx / 21] = 1;
				q->a = idx;
				q->b = ~idx;
				q->c = (uint32_t)i;
			}
		}
		for (i = 0; i < 315; i++) {
			p = genradix_ptr(&gnz, (uint64_t)i);
			CHECK((p != NULL) == (zpage[i / 21] != 0),
			      "gobj idx %d presence mismatch", i);
		}
		it = genradix_iter_init(&gnz, 0);
		while ((p = genradix_iter_peek(&it, &gnz)) != NULL) {
			CHECK(it.pos < 315 && zpage[it.pos / 21],
			      "gobj iter pos %llu in free page",
			      (unsigned long long)it.pos);
			CHECK(p->b == ~p->a || p->a == 0,
			      "gobj value corrupt at %llu",
			      (unsigned long long)it.pos);
			genradix_iter_advance(&it, &gnz);
		}
		genradix_free(&gnz);
	}
}

/* ---- interval tree ---- */

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

/* recompute expected __subtree_last of the subtree rooted at rb */
static unsigned long subtree_max(struct rb_node *rb)
{
	unsigned long m = itn_of_rb(rb)->n.last, t;
	if (rb->rb_left && (t = subtree_max(rb->rb_left)) > m)
		m = t;
	if (rb->rb_right && (t = subtree_max(rb->rb_right)) > m)
		m = t;
	return m;
}

static void validate_augment(struct rb_node *rb)
{
	if (!rb)
		return;
	CHECK(itn_of_rb(rb)->n.__subtree_last == subtree_max(rb),
	      "node %d __subtree_last %lu want %lu",
	      (int)(itn_of_rb(rb) - ipool),
	      itn_of_rb(rb)->n.__subtree_last, subtree_max(rb));
	validate_augment(rb->rb_left);
	validate_augment(rb->rb_right);
}

static int intersects(const struct interval_tree_node *n,
		      unsigned long s, unsigned long l)
{
	return n->start <= l && s <= n->last;
}

/* expected in-order sequence of intersecting nodes, via raw rb links */
static void expected_seq(struct rb_node *rb, unsigned long s,
			 unsigned long l, int *out, int *k)
{
	if (!rb)
		return;
	expected_seq(rb->rb_left, s, l, out, k);
	if (intersects(&itn_of_rb(rb)->n, s, l))
		out[(*k)++] = (int)(itn_of_rb(rb) - ipool);
	expected_seq(rb->rb_right, s, l, out, k);
}

static void check_query(unsigned long s, unsigned long l)
{
	int exp[ITN], k = 0, i = 0;
	struct interval_tree_node *p;

	expected_seq(itree.rb_root.rb_node, s, l, exp, &k);
	for (p = interval_tree_iter_first(&itree, s, l); p;
	     p = interval_tree_iter_next(p, s, l)) {
		CHECK(i < k, "iter yielded extra node %d",
		      (int)(container_of(p, struct itnode, n) - ipool));
		if (i < k)
			CHECK((int)(container_of(p, struct itnode, n) - ipool)
			      == exp[i], "iter order mismatch at %d", i);
		i++;
	}
	CHECK(i == k, "iter yielded %d nodes want %d", i, k);
}

static void check_spans(unsigned long first, unsigned long last)
{
	struct interval_tree_span_iter span;
	unsigned long at = first;
	int want_hole = -1; /* first span may be either */

	for (interval_tree_span_iter_first(&span, &itree, first, last);
	     !interval_tree_span_iter_done(&span);
	     interval_tree_span_iter_next(&span)) {
		unsigned long a, b;
		int i;
		if (span.is_hole) {
			a = span.start_hole;
			b = span.last_hole;
		} else {
			a = span.start_used;
			b = span.last_used;
		}
		CHECK(a == at, "span gap: starts %lu expected %lu", a, at);
		CHECK(a <= b && b <= last, "bad span [%lu,%lu]", a, b);
		/* brute-force coverage check */
		for (i = 0; i < ITN; i++) {
			if (!ipool[i].present)
				continue;
			if (span.is_hole)
				CHECK(!intersects(&ipool[i].n, a, b),
				      "node %d intersects hole", i);
		}
		at = b + 1;
	}
	CHECK(at == last + 1, "spans ended at %lu want %lu",
	      at - 1, last);
}

static void interval_test(void)
{
	int seed, i;
	for (seed = 1; seed <= 25; seed++) {
		memset(ipool, 0, sizeof ipool);
		itree = RB_ROOT_CACHED;
		rng_state = seed;
		for (i = 0; i < 300; i++) {
			uint64_t r = xs64();
			int idx = (int)(xs64() % ITN);
			if (r % 100 < 55) {
				if (!ipool[idx].present) {
					ipool[idx].n.start = xs64() % ITKEY;
					ipool[idx].n.last =
						ipool[idx].n.start +
						xs64() % 96;
					interval_tree_insert(&ipool[idx].n,
							   &itree);
					ipool[idx].present = 1;
				}
			} else if (ipool[idx].present) {
				interval_tree_remove(&ipool[idx].n, &itree);
				ipool[idx].present = 0;
			}
			validate_augment(itree.rb_root.rb_node);
		}
		for (i = 0; i < 60; i++) {
			unsigned long s = xs64() % ITKEY;
			unsigned long l = s + xs64() % 160;
			check_query(s, l);
		}
		for (i = 0; i < 20; i++) {
			unsigned long s = xs64() % ITKEY;
			unsigned long l = s + xs64() % 160;
			check_spans(s, l);
		}
		check_spans(0, ITKEY - 1);
	}
}

int main(void)
{
	genradix_test();
	interval_test();
	printf("%s (%d failures)\n", fails ? "FAILURES" : "ALL OK", fails);
	return fails ? 1 : 0;
}
