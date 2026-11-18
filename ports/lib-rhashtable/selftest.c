/* Hand-computed invariant self-test (not part of the shipped port):
 * drives the real C code directly and checks API contracts that were
 * worked out by hand.  Exit 0 = all good. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <linux/types.h>
#include <linux/compiler.h>
#include <linux/kernel.h>
#include <linux/slab.h>
#include <linux/random.h>
#include <linux/mm.h>
#include <linux/err.h>
#include <linux/errno.h>
#include <linux/jhash.h>
#include <linux/rhashtable.h>
#include <linux/spinlock.h>

#define N 32

struct tobj {
	uint64_t meta;
	struct rhlist_head rhl;
	uint64_t key;
	uint64_t tag;
	int present;
};

#define HEAD_OFF ((uint16_t)offsetof(struct tobj, rhl))
#define KEY_OFF  ((uint16_t)offsetof(struct tobj, key))

static struct tobj pool[N];
static int fails;

#define CHECK(c, ...) do { if (!(c)) { fails++; \
	printf("FAIL %d: ", __LINE__); printf(__VA_ARGS__); \
	printf("\n"); } } while (0)

/* ---- shim backends are provided by wrappers.c ---- */
extern void *kshim_alloc(size_t size, gfp_t gfp);
extern void kshim_free(const void *p);
extern u32 kshim_random_u32(void);
extern unsigned int num_possible_cpus(void);

static struct tobj *find(struct rhashtable *ht, uint64_t k,
			 struct rhashtable_params p)
{
	return rhashtable_lookup_fast(ht, &k, p);
}

static int nfree;
static void count_free(void *ptr, void *arg)
{
	(void)arg;
	nfree++;
}

int main(void)
{
	struct rhashtable ht;
	struct rhltable hlt;
	struct rhashtable_params p;
	struct rhashtable_iter it;
	struct bucket_table *t;
	uint64_t keys[4] = { 11, 222, 3333, 44444 };
	void *o;
	int i, ret, cnt;

	kshim_random_u32();	/* seed shim rng deterministically */

	/* ---- rhashtable: init/insert/lookup/remove/destroy ---- */
	memset(&p, 0, sizeof p);
	p.key_len = 8;
	p.key_offset = KEY_OFF;
	p.head_offset = HEAD_OFF;
	p.min_size = 4;

	memset(&ht, 0, sizeof ht);
	ret = rhashtable_init(&ht, &p);
	CHECK(ret == 0, "init ret=%d", ret);
	t = rcu_dereference_raw(ht.tbl);
	CHECK(t && t->size == 64, "default size=%u (want 64)", t->size);
	CHECK(ht.key_len == 2, "key_len halved to %u (want 2 u32s)",
	      ht.key_len);
	CHECK(atomic_read(&ht.nelems) == 0, "nelems=%d",
	      atomic_read(&ht.nelems));

	/* insert 3, lookup each, remove 1, lookup absent == NULL */
	for (i = 0; i < 3; i++) {
		pool[i].key = keys[i];
		ret = rhashtable_lookup_insert_fast(&ht, &pool[i].rhl.rhead,
						  p);
		CHECK(ret == 0, "insert %d ret=%d", i, ret);
	}
	CHECK(atomic_read(&ht.nelems) == 3, "nelems=%d",
	      atomic_read(&ht.nelems));

	/* duplicate insert must fail -EEXIST */
	pool[3].key = keys[0];
	ret = rhashtable_lookup_insert_fast(&ht, &pool[3].rhl.rhead, p);
	CHECK(ret == -EEXIST, "dup insert ret=%d", ret);

	for (i = 0; i < 3; i++) {
		o = find(&ht, keys[i], p);
		CHECK(o == &pool[i], "lookup %d -> %p (want %p)", i, o,
		      &pool[i]);
	}
	o = find(&ht, 999999, p);
	CHECK(o == NULL, "absent lookup -> %p", o);

	ret = rhashtable_remove_fast(&ht, &pool[1].rhl.rhead, p);
	CHECK(ret == 0, "remove ret=%d", ret);
	CHECK(atomic_read(&ht.nelems) == 2, "nelems=%d",
	      atomic_read(&ht.nelems));
	CHECK(find(&ht, keys[1], p) == NULL, "removed key found");
	ret = rhashtable_remove_fast(&ht, &pool[1].rhl.rhead, p);
	CHECK(ret == -ENOENT, "re-remove ret=%d", ret);

	/* replace */
	pool[4].key = keys[0];
	ret = rhashtable_replace_fast(&ht, &pool[0].rhl.rhead,
				      &pool[4].rhl.rhead, p);
	CHECK(ret == 0, "replace ret=%d", ret);
	CHECK(find(&ht, keys[0], p) == &pool[4], "replace lookup wrong");
	pool[5].key = 77777;	/* different key -> -EINVAL */
	ret = rhashtable_replace_fast(&ht, &pool[4].rhl.rhead,
				      &pool[5].rhl.rhead, p);
	CHECK(ret == -EINVAL, "replace EINVAL ret=%d", ret);

	/* walk: count elements == 2 */
	rhashtable_walk_enter(&ht, &it);
	rhashtable_walk_start(&it);
	cnt = 0;
	while ((o = rhashtable_walk_next(&it)))
		cnt++;
	rhashtable_walk_stop(&it);
	rhashtable_walk_exit(&it);
	CHECK(cnt == 2, "walk saw %d (want 2)", cnt);

	nfree = 0;
	rhashtable_free_and_destroy(&ht, count_free, NULL);
	CHECK(nfree == 2, "destroy freed %d (want 2)", nfree);

	/* ---- rhltable: duplicate keys form per-key lists ---- */
	memset(&p, 0, sizeof p);
	p.key_len = 8;
	p.key_offset = KEY_OFF;
	p.head_offset = HEAD_OFF;
	p.min_size = 4;

	memset(&hlt, 0, sizeof hlt);
	ret = rhltable_init(&hlt, &p);
	CHECK(ret == 0, "hlt init ret=%d", ret);
	CHECK(hlt.ht.rhlist, "rhlist flag not set");

	for (i = 0; i < 4; i++) {	/* two dups of keys[0], one of keys[1] */
		pool[i].key = (i < 3) ? keys[0] : keys[1];
		ret = rhltable_insert(&hlt, &pool[i].rhl, p);
		CHECK(ret == 0, "rhl insert %d ret=%d", i, ret);
	}
	/* rhlist tables count one nelem per key group, not per object */
	CHECK(atomic_read(&hlt.ht.nelems) == 2, "hlt nelems=%d",
	      atomic_read(&hlt.ht.nelems));
	{
		struct rhlist_head *l, *pos;
		struct tobj *to;
		int c = 0;
		uint64_t k = keys[0];
		l = rhltable_lookup(&hlt, &k, p);
		CHECK(l != NULL, "rhl lookup miss");
		rhl_for_each_entry_rcu(to, pos, l, rhl)
			c++;
		CHECK(c == 3, "dup list len %d (want 3)", c);
	}
	ret = rhltable_remove(&hlt, &pool[1].rhl, p);
	CHECK(ret == 0, "rhl remove ret=%d", ret);
	{
		struct rhlist_head *l, *pos;
		struct tobj *to;
		int c = 0;
		uint64_t k = keys[0];
		l = rhltable_lookup(&hlt, &k, p);
		CHECK(l != NULL, "rhl lookup2 miss");
		rhl_for_each_entry_rcu(to, pos, l, rhl)
			c++;
		CHECK(c == 2, "dup list len %d (want 2)", c);
	}
	nfree = 0;
	rhltable_free_and_destroy(&hlt, count_free, NULL);
	CHECK(nfree == 3, "hlt destroy freed %d (want 3)", nfree);

	/* ---- bucket_locks ---- */
	{
		spinlock_t *locks = NULL;
		unsigned int mask = 0;
		ret = alloc_bucket_spinlocks(&locks, &mask, 100, 0,
					     GFP_KERNEL);
		CHECK(ret == 0 && mask == 99, "bkl ret=%d mask=%u", ret,
		      mask);
		free_bucket_spinlocks(locks);
		/* cpu_mult: min(8 cpus * mult, max_size) */
		ret = alloc_bucket_spinlocks(&locks, &mask, 10, 4,
					     GFP_KERNEL);
		CHECK(ret == 0 && mask == 9, "bkl cm ret=%d mask=%u",
		      ret, mask);
		free_bucket_spinlocks(locks);
	}

	printf("%s (%d failures)\n", fails ? "FAILURES" : "ALL OK", fails);
	return fails ? 1 : 0;
}
