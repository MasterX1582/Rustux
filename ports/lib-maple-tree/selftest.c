/* Invariant self-test (not part of the shipped port): a brute-force
 * shadow model of sorted disjoint ranges is maintained beside the real
 * tree.  After every op: load() probes agree with the shadow, found
 * ranges' endpoints agree with the shadow, empty-area answers contain
 * no shadow-covered index, and the node/sheaf pools never leak across
 * mtree_destroy/mas_destroy.  Exit 0 = all good. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <linux/types.h>
#include <linux/lockdep.h>
#include <linux/maple_tree.h>
#include <linux/slab.h>
#include <linux/errno.h>
#include <linux/gfp.h>

#define NP 448
#define EP 96
#define NSP 12

struct slab_sheaf {
	unsigned int size;
	struct maple_node *objs[32];
};

static struct maple_node node_pool[NP] __attribute__((aligned(256)));
static uint8_t node_used[NP];
static struct slab_sheaf sheaf_pool[NSP];
static uint8_t sheaf_used[NSP];
static uint64_t epool[EP] __attribute__((aligned(64)));
static struct maple_tree mt;
struct lockdep_map rcu_callback_map;
unsigned long kshim_warn_hits;
static unsigned long alloc_ctr, fail_mask;

static struct kmem_cache the_cache;

static void *node_alloc(gfp_t gfp)
{
	int i;
	(void)gfp;
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

static void node_free(void *p)
{
	struct maple_node *n = p;
	if (!n)
		return;
	node_used[n - node_pool] = 0;
}

struct kmem_cache *__kmem_cache_create_args(const char *name,
					    unsigned int size,
					    struct kmem_cache_args *args,
					    slab_flags_t flags)
{
	(void)name; (void)flags;
	the_cache.obj_size = size;
	the_cache.align = args ? args->align : 0;
	the_cache.sheaf_capacity = args ? args->sheaf_capacity : 0;
	return &the_cache;
}

void *kmem_cache_alloc(struct kmem_cache *cachep, gfp_t gfp)
{
	(void)cachep;
	return node_alloc(gfp);
}

void kmem_cache_free(struct kmem_cache *cachep, void *obj)
{
	(void)cachep;
	node_free(obj);
}

void kfree(const void *objp)
{
	node_free((void *)objp);
}

void kmem_cache_free_bulk(struct kmem_cache *s, size_t size, void **p)
{
	size_t i;
	(void)s;
	for (i = 0; i < size; i++)
		node_free(p[i]);
}

struct slab_sheaf *kmem_cache_prefill_sheaf(struct kmem_cache *s,
					    gfp_t gfp, unsigned int size)
{
	struct slab_sheaf *sh;
	unsigned int i, j;
	(void)s;
	alloc_ctr++;
	if (alloc_ctr <= 64 && (fail_mask >> (alloc_ctr - 1)) & 1)
		return NULL;
	for (i = 0; i < NSP; i++)
		if (!sheaf_used[i])
			break;
	if (i == NSP)
		return NULL;
	sheaf_used[i] = 1;
	sh = &sheaf_pool[i];
	sh->size = 0;
	for (i = 0; i < size && i < 32; i++) {
		struct maple_node *n = node_alloc(gfp);
		if (!n) {
			for (j = 0; j < i; j++)
				node_free(sh->objs[j]);
			sheaf_used[sh - sheaf_pool] = 0;
			return NULL;
		}
		sh->objs[i] = n;
	}
	sh->size = i;
	return sh;
}

int kmem_cache_refill_sheaf(struct kmem_cache *s, gfp_t gfp,
			    struct slab_sheaf **sheaf, unsigned int size)
{
	struct slab_sheaf *sh = *sheaf;
	(void)s;
	while (sh->size < size && sh->size < 32) {
		struct maple_node *n = node_alloc(gfp);
		if (!n)
			return -ENOMEM;
		sh->objs[sh->size++] = n;
	}
	return 0;
}

void kmem_cache_return_sheaf(struct kmem_cache *s, gfp_t gfp,
			     struct slab_sheaf *sheaf)
{
	unsigned int i;
	(void)s; (void)gfp;
	for (i = 0; i < sheaf->size; i++)
		node_free(sheaf->objs[i]);
	sheaf_used[sheaf - sheaf_pool] = 0;
}

void *kmem_cache_alloc_from_sheaf(struct kmem_cache *cachep, gfp_t gfp,
				  struct slab_sheaf *sheaf)
{
	(void)cachep; (void)gfp;
	if (!sheaf->size) {
		kshim_warn_hits++;
		return NULL;
	}
	return sheaf->objs[--sheaf->size];
}

unsigned int kmem_cache_sheaf_size(struct slab_sheaf *sheaf)
{
	return sheaf->size;
}

/* ---- shadow model ---- */

struct range_s {
	unsigned long lo, hi;
	void *e;
};
static struct range_s shadow[512];
static int nshadow;

static void *shadow_lookup(unsigned long i)
{
	int k;
	for (k = 0; k < nshadow; k++)
		if (i >= shadow[k].lo && i <= shadow[k].hi)
			return shadow[k].e;
	return NULL;
}

/* is every index in [lo,hi] covered by shadow entry e? */
static int shadow_covers(unsigned long lo, unsigned long hi, void *e)
{
	int k;
	for (k = 0; k < nshadow; k++)
		if (shadow[k].lo <= lo && shadow[k].hi >= hi)
			return shadow[k].e == e;
	return 0;
}

static int shadow_free(unsigned long lo, unsigned long hi)
{
	int k;
	for (k = 0; k < nshadow; k++)
		if (shadow[k].lo <= hi && lo <= shadow[k].hi)
			return 0;
	return 1;
}

static void shadow_store(unsigned long lo, unsigned long hi, void *e)
{
	struct range_s tmp[512];
	int k, n = 0;
	/* split out the window */
	for (k = 0; k < nshadow; k++) {
		struct range_s r = shadow[k];
		if (r.hi < lo || r.lo > hi) {
			tmp[n++] = r;
			continue;
		}
		if (r.lo < lo) {
			struct range_s a = { r.lo, lo - 1, r.e };
			tmp[n++] = a;
		}
		if (r.hi > hi) {
			struct range_s b = { hi + 1, r.hi, r.e };
			tmp[n++] = b;
		}
	}
	if (e) {
		struct range_s r = { lo, hi, e };
		tmp[n++] = r;
	}
	/* sort by lo (insertion) and merge adjacent equal entries */
	for (k = 0; k < n; k++) {
		int m, j;
		for (m = k + 1; m < n; m++)
			if (tmp[m].lo < tmp[k].lo) {
				struct range_s t = tmp[k];
				tmp[k] = tmp[m];
				tmp[m] = t;
			}
	}
	nshadow = 0;
	for (k = 0; k < n; k++) {
		if (nshadow && shadow[nshadow - 1].e == tmp[k].e &&
		    shadow[nshadow - 1].hi + 1 == tmp[k].lo &&
		    tmp[k].lo != 0) {
			shadow[nshadow - 1].hi = tmp[k].hi;
			continue;
		}
		shadow[nshadow++] = tmp[k];
	}
}

/* ---- checks ---- */

static int fails;
#define CHECK(c, ...) do { if (!(c)) { fails++; \
	printf("FAIL %d: ", __LINE__); printf(__VA_ARGS__); \
	printf("\n"); if (fails > 20) exit(1); } } while (0)

static int pool_leaks(void)
{
	int i, n = 0;
	for (i = 0; i < NP; i++)
		n += node_used[i];
	for (i = 0; i < NSP; i++)
		n += sheaf_used[i];
	return n;
}

/* probe a few indices around the shadow-covered region */
static void probe_loads(void)
{
	int k;
	for (k = 0; k < nshadow; k++) {
		unsigned long lo = shadow[k].lo, hi = shadow[k].hi;
		unsigned long probes[6];
		int np = 0, j;
		probes[np++] = lo;
		if (hi != lo)
			probes[np++] = hi;
		probes[np++] = lo + (hi - lo) / 2;
		if (lo)
			probes[np++] = lo - 1;
		if (hi != ULONG_MAX)
			probes[np++] = hi + 1;
		for (j = 0; j < np; j++)
			CHECK(mtree_load(&mt, probes[j]) ==
			      shadow_lookup(probes[j]),
			      "load %lx: tree %p shadow %p",
			      probes[j], mtree_load(&mt, probes[j]),
			      shadow_lookup(probes[j]));
	}
}

static uint64_t rng_state;
static uint64_t xs64(void)
{
	uint64_t x = rng_state;
	x ^= x >> 12; x ^= x << 25; x ^= x >> 27;
	rng_state = x;
	return x * 2685821657736338717ULL;
}

static void *pentry(int i)
{
	return &epool[i % EP];
}

int main(void)
{
	int i;

	printf("maple selftest\n");

	/* ---- hand-computed mini sequences ---- */
	maple_tree_init();
	mt_init(&mt);
	CHECK(mtree_empty(&mt), "empty tree not empty");
	CHECK(mtree_load(&mt, 0) == NULL, "empty load");
	CHECK(mtree_store(&mt, 5, pentry(1), GFP_KERNEL) == 0, "store 5");
	CHECK(mtree_load(&mt, 5) == pentry(1), "load 5");
	CHECK(mtree_load(&mt, 4) == NULL, "load 4 hole");
	CHECK(mtree_store_range(&mt, 10, 20, pentry(2), GFP_KERNEL) == 0,
	      "store range");
	CHECK(mtree_load(&mt, 15) == pentry(2), "load 15");
	CHECK(mtree_erase(&mt, 5) == pentry(1), "erase 5");
	CHECK(mtree_load(&mt, 5) == NULL, "post-erase 5");
	CHECK(mtree_insert(&mt, 10, pentry(3), GFP_KERNEL) == -EEXIST,
	      "insert exists");
	CHECK(mtree_insert(&mt, 30, pentry(3), GFP_KERNEL) == 0,
	      "insert 30");
	mtree_destroy(&mt);
	CHECK(mtree_empty(&mt), "destroyed not empty");
	CHECK(pool_leaks() == 0, "pool leak after destroy: %d", pool_leaks());

	/* ---- seeded shadow-model run ---- */
	for (int flags_i = 0; flags_i < 2; flags_i++) {
		mt_init_flags(&mt, flags_i ? MT_FLAGS_ALLOC_RANGE : 0);
		nshadow = 0;
		rng_state = 0xbeefULL + flags_i;
		fail_mask = 0;
		for (i = 0; i < 300; i++) {
			uint64_t r = xs64();
			unsigned long s = (r >> 8) % 2000;
			unsigned long l = s + (r >> 20) % 60;
			void *e;
			switch (r % 10) {
			case 0: case 1: case 2: case 3:
				e = (r & 0x100) ? pentry((int)(r >> 30))
						: NULL;
				if (mtree_store_range(&mt, s, l, e,
						      GFP_KERNEL) == 0)
					shadow_store(s, l, e);
				break;
			case 4: case 5:
				e = mtree_erase(&mt, s);
				CHECK(e == shadow_lookup(s),
				      "erase %lx ret %p shadow %p", s, e,
				      shadow_lookup(s));
				if (shadow_lookup(s)) {
					/* erase removes the WHOLE range */
					int k;
					for (k = 0; k < nshadow; k++)
						if (s >= shadow[k].lo &&
						    s <= shadow[k].hi)
							shadow_store(shadow[k].lo,
								shadow[k].hi,
								NULL);
				}
				break;
			case 6: {
				MA_STATE(mas, &mt, s, s);
				e = mas_walk(&mas);
				CHECK(e == shadow_lookup(s),
				      "walk %lx ret %p shadow %p", s, e,
				      shadow_lookup(s));
				if (e)
					CHECK(shadow_covers(mas.index,
							    mas.last, e),
					      "walk range %lx-%lx", mas.index,
					      mas.last);
				break;
			}
			case 7: {
				MA_STATE(mas, &mt, 0, 0);
				int ret;
				if (flags_i) {
					ret = mas_empty_area(&mas, s,
							     s + 500,
							     1 + r % 16);
					if (!ret)
						CHECK(shadow_free(mas.index,
							mas.index +
							1 + r % 16 - 1),
						      "empty_area %lx",
						      mas.index);
				}
				mas_destroy(&mas);
				break;
			}
			case 8: {
				unsigned long idx = s;
				e = mt_find(&mt, &idx, s + 300);
				if (e) {
					/* idx is advanced to mas.last + 1
					 * (just past the found range) */
					CHECK(idx &&
					      shadow_lookup(idx - 1) == e,
					      "find %lx entry", idx);
					CHECK(idx &&
					      mtree_load(&mt, idx - 1) == e,
					      "find load");
				}
				break;
			}
			default: {
				/* find next occupied >= s via shadow */
				MA_STATE(mas, &mt, s, s);
				e = mas_find(&mas, ULONG_MAX);
				if (e) {
					CHECK(shadow_covers(mas.index,
							    mas.last, e),
					      "find range %lx-%lx e=%p",
					      mas.index, mas.last, e);
				} else {
					int k, any = 0;
					for (k = 0; k < nshadow; k++)
						if (shadow[k].hi >= s)
							any = 1;
					CHECK(!any, "find NULL but shadow "
					      "has >= %lx", s);
				}
				break;
			}
			}
			if (i % 20 == 19)
				probe_loads();
		}
		probe_loads();
		mtree_destroy(&mt);
		CHECK(pool_leaks() == 0, "pool leak flags=%d: %d",
		      flags_i, pool_leaks());
	}

	/* ---- alloc-failure storm: no leaks, tree still coherent ---- */
	mt_init(&mt);
	nshadow = 0;
	rng_state = 0xf00dULL;
	for (int round = 0; round < 8; round++) {
		fail_mask = (round & 1) ? ~0ULL : 0x15ULL;
		for (i = 0; i < 30; i++) {
			uint64_t r = xs64();
			unsigned long s = (r >> 8) % 500;
			unsigned long l = s + (r >> 20) % 30;
			void *e = pentry((int)(r >> 30));
			int ret = mtree_store_range(&mt, s, l, e, GFP_KERNEL);
			if (!ret)
				shadow_store(s, l, e);
		}
		probe_loads();
	}
	alloc_ctr = 200;	/* disable fail injection */
	mtree_destroy(&mt);
	CHECK(pool_leaks() == 0, "leak after failstorm: %d", pool_leaks());

	if (fails) {
		printf("SELFTEST FAIL (%d)\n", fails);
		return 1;
	}
	printf("SELFTEST PASS\n");
	return 0;
}
