/* Scalar wrappers around lib/rhashtable.c, lib/bucket_locks.c and the
 * include/linux/{rhashtable,rhashtable-types,jhash,list_nulls}.h layer,
 * so the standard scalar bridge can drive them.
 *
 * Address model: all kernel objects live in static pools / a bump arena,
 * and every wrapper returns one order-sensitive FNV-1a checksum folded
 * over CANONICAL addresses, never raw pointers:
 *   canon(NULL)              = 0
 *   canon(nulls marker p|1)  = canon(p) | 1
 *   canon(&pool[i] + off)    = POOLV + i*48 + off    (tobj slot)
 *   canon(arena alloc p)     = ARENAV + (p - arena)  (bump offset)
 *   canon(&rhnull / other)   = RHNULLV
 * The bump allocator never reuses memory, so "freed" tables stay
 * readable like the RCU-deferred frees the algorithm expects, and the
 * offsets are identical on the Rust side.
 *
 * struct tobj { u64 meta; rhlist_head rhl; u64 key; u64 tag; present }
 * is reached through container_of() exactly like real users:
 * head_offset = offsetof(tobj, rhl) = 8, key_offset = 24.
 *
 * gfp failures are injected by allocation ordinal: the g_fail_nth-th
 * kshim_alloc() attempt returns NULL (__GFP_NOFAIL allocs are exempt,
 * mirroring the kernel's retry semantics).
 */
#include <stdint.h>
#include <stddef.h>
#include <string.h>
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

#define N		1024
#define NI		2
#define NILX		0xffffu
#define KEYSPACE	(4u * N)
#define ARENA_SZ	(64u << 20)
#define POOLV		0x1000ULL
#define ARENAV		0x1000000ULL
#define RHNULLV		0x700000000000ULL
#define DRAIN_MAX	8

struct tobj {
	uint64_t		meta;
	struct rhlist_head	rhl;
	uint64_t		key;
	uint64_t		tag;
	int			present;
	int			freed;
};

#define HEAD_OFF	((uint16_t)offsetof(struct tobj, rhl))
#define KEY_OFF		((uint16_t)offsetof(struct tobj, key))

static struct tobj pool[N];
static struct rhashtable_iter iters[NI];
static int iter_entered[NI];
static int walk_on[NI];
static struct rhashtable ht;
static struct rhltable hlt;
static struct rhashtable_params g_par;
static int ht_live;
static int hlt_live;

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

/* ---- kernel service hooks (shim backends) ---- */

static unsigned char arena[ARENA_SZ] __attribute__((aligned(4096)));
static size_t arena_off;
uint64_t g_alloc_ord;
static uint64_t g_fail_nth;
static uint64_t g_rnd;
unsigned long g_alloc_ord_done;	/* alloc ordinal at end of last op */

void *kshim_alloc(size_t size, gfp_t gfp)
{
	size_t a;
	void *p;

	g_alloc_ord++;
	if (g_alloc_ord == g_fail_nth && !(gfp & __GFP_NOFAIL))
		return NULL;
	a = (arena_off + 7) & ~(size_t)7;
	if (a + size > ARENA_SZ)
		return NULL;
	arena_off = a + size;
	p = arena + a;
	if (gfp & __GFP_ZERO)
		memset(p, 0, size);
	return p;
}

void kshim_free(const void *p)
{
	(void)p;	/* bump arena: freed memory stays readable */
}

u32 kshim_random_u32(void)
{
	g_rnd ^= g_rnd >> 12;
	g_rnd ^= g_rnd << 25;
	g_rnd ^= g_rnd >> 27;
	g_rnd *= 2685821657736338717ULL;
	return (u32)(g_rnd >> 32);
}

unsigned int num_possible_cpus(void)
{
	return 8;
}

/* canonical address fold */
static uint64_t canon(const void *p)
{
	uintptr_t u = (uintptr_t)p;

	if (!u)
		return 0;
	if (u & 1)
		return canon((void *)(u & ~1)) | 1;
	if (u >= (uintptr_t)pool && u < (uintptr_t)&pool[N])
		return POOLV + (u - (uintptr_t)pool);
	if (u >= (uintptr_t)arena && u < (uintptr_t)(arena + ARENA_SZ))
		return ARENAV + (u - (uintptr_t)arena);
	return RHNULLV;
}

static int tidx(const void *p)
{
	const struct tobj *o = p;
	return o ? (int)(o - pool) : (int)NILX;
}

/* ---- params / hash-fn variants ---- */

static u32 my_hashfn(const void *data, u32 len, u32 seed)
{
	return jhash(data, len, seed) ^ 0x9e3779b9u;
}

static u32 obj_hash_good(const void *data, u32 len, u32 seed)
{
	const struct tobj *o = data;
	(void)len;
	return jhash(&o->key, sizeof(o->key), seed);
}

static u32 obj_hash_bad(const void *data, u32 len, u32 seed)
{
	const struct tobj *o = data;
	(void)len;
	(void)seed;
	return (u32)(o->key & 0xf);
}

static int obj_cmp_eq(struct rhashtable_compare_arg *arg, const void *obj)
{
	const struct tobj *o = obj;
	return o->key != *(const uint64_t *)arg->key;
}

static int obj_cmp_lo32(struct rhashtable_compare_arg *arg, const void *obj)
{
	const struct tobj *o = obj;
	return (u32)o->key != (u32)*(const uint64_t *)arg->key;
}

static struct rhashtable_params mk_params(uint64_t v, int rhlist)
{
	struct rhashtable_params p;
	static const uint16_t hints[] = { 0, 24, 48, 64 };
	static const uint16_t keylens[] = { 8, 5, 4, 8 };
	static const unsigned int maxsz[] = { 0, 64, 256, 4096 };
	static const uint16_t minsz[] = { 0, 4, 8, 16 };
	uint64_t m = v % 64;

	memset(&p, 0, sizeof p);
	p.nelem_hint = hints[v % 4];
	p.key_len = keylens[(v >> 2) % 4];
	p.key_offset = KEY_OFF;
	p.head_offset = HEAD_OFF;
	p.max_size = maxsz[(v >> 4) % 4];
	p.min_size = minsz[(v >> 6) % 4];
	p.insecure_elasticity = !!(v & 0x200);
	p.automatic_shrinking = !!(v & 0x100);
	if (v & 0x400)
		p.hashfn = my_hashfn;
	if (rhlist || (m % 5 == 3)) {
		p.obj_hashfn = (v & 0x800) ? obj_hash_bad : obj_hash_good;
		p.obj_cmpfn = (v & 0x1000) ? obj_cmp_lo32 : obj_cmp_eq;
	}
	return p;
}

/* ---- shared helpers ---- */

static void reset_all(uint64_t seed, uint64_t fail_i)
{
	memset(pool, 0, sizeof pool);
	memset(iters, 0, sizeof iters);
	memset(iter_entered, 0, sizeof iter_entered);
	memset(walk_on, 0, sizeof walk_on);
	memset(&ht, 0, sizeof ht);
	memset(&hlt, 0, sizeof hlt);
	memset(&g_par, 0, sizeof g_par);
	ht_live = 0;
	hlt_live = 0;
	arena_off = 0;
	g_alloc_ord = 0;
	g_fail_nth = iabs(fail_i) % 97;
	g_rnd = (seed ^ 0x517cc1b727220a95ULL) | 1;
	rng_state = seed | 1;
	g_h = 0x9e3779b97f4a7c15ULL;
}

static int free_slot(void)
{
	int i;
	for (i = 0; i < N; i++)
		if (!pool[i].present && !pool[i].freed)
			return i;
	return -1;
}

static int nth_present(uint64_t n)
{
	int i;
	for (i = 0; i < N; i++)
		if (pool[i].present && !pool[i].freed && !n--)
			return i;
	return -1;
}

static int count_present(void)
{
	int i, c = 0;
	for (i = 0; i < N; i++)
		if (pool[i].present && !pool[i].freed)
			c++;
	return c;
}

/* free_fn for free_and_destroy: records release order */
static void free_cb(void *ptr, void *arg)
{
	struct tobj *o = ptr;
	(void)arg;
	fold(tidx(o));
	o->present = 0;
	o->freed = 1;
}

/* drain queued work: the async resize worker run synchronously */
static void drain_work(struct rhashtable *t)
{
	int n = 0;
	while (n < DRAIN_MAX) {
		if (t->run_irq_work.queued) {
			t->run_irq_work.queued = 0;
			t->run_irq_work.func(&t->run_irq_work);
			n++;
			continue;
		}
		if (t->run_work.queued) {
			t->run_work.queued = 0;
			t->run_work.func(&t->run_work);
			n++;
			continue;
		}
		break;
	}
	fold(0xdeed0000 | (uint64_t)n);
}

/* ---- structure dumps ---- */

static void fold_chain(struct bucket_table *tbl, unsigned int b, int rhlist)
{
	struct rhash_lock_head __rcu *const *bkt = rht_bucket(tbl, b);
	struct rhash_head *pos;

	for (pos = rht_ptr(bkt, tbl, b);
	     !rht_is_a_nulls(pos);
	     pos = rcu_dereference_all(pos->next)) {
		fold(canon(pos));
		if (rhlist) {
			struct rhlist_head *l =
				container_of(pos, struct rhlist_head, rhead);
			while (l) {
				fold(0x1a);
				fold(canon(l));
				l = rcu_dereference_all(l->next);
			}
			fold(0x1b);
		}
	}
	fold(canon(pos));	/* the terminating nulls marker */
}

static int walker_count(struct bucket_table *tbl)
{
	struct rhashtable_walker *w;
	int c = 0;
	list_for_each_entry(w, &tbl->walkers, list)
		c++;
	return c;
}

static void dump_ht(struct rhashtable *t)
{
	struct bucket_table *tbl;
	int k = 0;

	fold(0xd09e0000);
	fold(atomic_read(&t->nelems));
	for (tbl = rcu_dereference_raw(t->tbl); tbl && k < 8;
	     tbl = rcu_dereference_raw(tbl->future_tbl), k++) {
		unsigned int b;
		fold(canon(tbl));
		fold(tbl->size);
		fold(tbl->nest);
		fold(tbl->hash_rnd);
		fold(walker_count(tbl));
		for (b = 0; b < tbl->size; b++)
			fold_chain(tbl, b, t->rhlist);
	}
	fold(0xd09e1111);
}

static void dump_iters(void)
{
	int i;
	fold(0x1e7e0000);
	for (i = 0; i < NI; i++) {
		fold(iters[i].slot);
		fold(iters[i].skip);
		fold(iters[i].end_of_table);
		fold(canon(iters[i].p));
		fold(canon(iters[i].list));
		fold(canon(iters[i].walker.tbl));
		fold(iter_entered[i]);
		fold(walk_on[i]);
	}
}

/* ---- op helpers ---- */

static void fold_insert_ret(void *data, void *new_tbl_or_null)
{
	(void)new_tbl_or_null;
	if (IS_ERR(data))
		fold((uint64_t)PTR_ERR(data));
	else if (!data)
		fold(0);
	else
		fold(canon(data));
}

static uint64_t key_of(int i)
{
	return pool[i].key;
}

/* choose a key: mostly fresh, sometimes an existing object's key */
static uint64_t pick_key(void)
{
	uint64_t r = xs64();
	if ((r & 3) == 0) {
		int c = count_present();
		if (c) {
			int i = nth_present((r >> 8) % c);
			if (i >= 0)
				return pool[i].key;
		}
	}
	return xs64() % KEYSPACE;
}

static int prep_obj(uint64_t key)
{
	int i = free_slot();
	if (i < 0)
		return -1;
	pool[i].meta = xs64();
	pool[i].key = key;
	pool[i].tag = xs64() % 1000;
	return i;
}

/* ---- walk ops (shared between rht and rhlt) ---- */

static void op_walk(struct rhashtable *t, uint64_t r, int rhlist)
{
	int k = (int)((r >> 16) % NI);
	uint64_t q = (r >> 8) % 8;

	if (!iter_entered[k] && q != 0)
		q = 0;			/* must enter first */
	switch (q) {
	case 0:	/* enter */
		if (iter_entered[k]) {
			fold(0xec);
			break;
		}
		rhashtable_walk_enter(t, &iters[k]);
		iter_entered[k] = 1;
		walk_on[k] = 0;
		fold(0xec00);
		break;
	case 1:	/* exit */
		if (walk_on[k])
			rhashtable_walk_stop(&iters[k]);
		rhashtable_walk_exit(&iters[k]);
		iter_entered[k] = 0;
		walk_on[k] = 0;
		fold(0xec01);
		break;
	case 2:	/* start_check */
		if (walk_on[k]) {
			fold(0xec02);
			break;
		}
		fold(0xec03);
		fold((uint64_t)(int64_t)rhashtable_walk_start_check(&iters[k]));
		walk_on[k] = 1;
		break;
	case 3:	/* start */
		if (walk_on[k]) {
			fold(0xec04);
			break;
		}
		rhashtable_walk_start(&iters[k]);
		walk_on[k] = 1;
		fold(0xec05);
		break;
	case 4:	/* next */
		if (!walk_on[k]) {
			fold(0xec06);
			break;
		}
		{
			void *o = rhashtable_walk_next(&iters[k]);
			fold(0xec07);
			if (IS_ERR(o))
				fold((uint64_t)PTR_ERR(o));
			else
				fold(canon(o));
		}
		break;
	case 5:	/* peek */
		if (!walk_on[k]) {
			fold(0xec08);
			break;
		}
		{
			void *o = rhashtable_walk_peek(&iters[k]);
			fold(0xec09);
			if (IS_ERR(o))
				fold((uint64_t)PTR_ERR(o));
			else
				fold(canon(o));
		}
		break;
	case 6:	/* stop */
		if (!walk_on[k]) {
			fold(0xec0a);
			break;
		}
		rhashtable_walk_stop(&iters[k]);
		walk_on[k] = 0;
		fold(0xec0b);
		break;
	default: /* restart: stop+start */
		if (!iter_entered[k]) {
			fold(0xec0c);
			break;
		}
		if (walk_on[k])
			rhashtable_walk_stop(&iters[k]);
		fold(0xec0d);
		fold((uint64_t)(int64_t)rhashtable_walk_start_check(&iters[k]));
		walk_on[k] = 1;
		break;
	}
	(void)rhlist;
	dump_iters();
}

/* ================= wrap_rht_seq: general op mix ================= */

uint64_t wrap_rht_seq(uint64_t seed, int64_t nops_i, int64_t fail_i)
{
	int nops = 40 + (int)(iabs(nops_i) % 480);
	int i;

	reset_all(seed, fail_i);
	g_par = mk_params(xs64(), 0);
	fold((uint64_t)(int64_t)rhashtable_init(&ht, &g_par));
	ht_live = 1;
	fold(atomic_read(&ht.nelems));
	{
		struct bucket_table *t = rcu_dereference_raw(ht.tbl);
		fold(t ? t->size : 0);
	}

	for (i = 0; i < nops; i++) {
		uint64_t r = xs64();
		int t = (int)(r % 100);

		if (!ht_live) {
			g_par = mk_params(xs64(), 0);
			fold((uint64_t)(int64_t)rhashtable_init(&ht, &g_par));
			ht_live = 1;
			continue;
		}

		if (t < 22) {		/* insert_fast: no dedup */
			int idx = prep_obj(xs64() % KEYSPACE);
			if (idx < 0) {
				fold(NILX);
			} else {
				int ret = rhashtable_insert_fast(
					&ht, &pool[idx].rhl.rhead, g_par);
				pool[idx].present = !ret;
				fold((uint64_t)(int64_t)ret);
				fold(idx);
			}
		} else if (t < 40) {	/* lookup_insert: dedup */
			uint64_t k = pick_key();
			int idx = prep_obj(k);
			if (idx < 0) {
				fold(NILX);
			} else {
				int ret;
				if (g_par.obj_hashfn)
					ret = rhashtable_lookup_insert_key(
						&ht, &pool[idx].key,
						&pool[idx].rhl.rhead, g_par);
				else
					ret = rhashtable_lookup_insert_fast(
						&ht, &pool[idx].rhl.rhead,
						g_par);
				pool[idx].present = !ret;
				fold((uint64_t)(int64_t)ret);
				fold(idx);
			}
		} else if (t < 50) {	/* lookup_get_insert */
			uint64_t k = pick_key();
			int idx = prep_obj(k);
			void *ret;
			if (idx < 0) {
				fold(NILX);
			} else {
				if (g_par.obj_hashfn)
					ret = rhashtable_lookup_get_insert_key(
						&ht, &pool[idx].key,
						&pool[idx].rhl.rhead, g_par);
				else
					ret = rhashtable_lookup_get_insert_fast(
						&ht, &pool[idx].rhl.rhead,
						g_par);
				pool[idx].present = !IS_ERR(ret) && !ret;
				if (IS_ERR(ret))
					fold((uint64_t)PTR_ERR(ret));
				else
					fold(canon(ret));
				fold(idx);
			}
		} else if (t < 60) {	/* remove_fast */
			int c = count_present();
			int idx;
			if (c && (r & 0x300)) {
				idx = nth_present((r >> 10) % c);
			} else {
				idx = (int)((r >> 10) % N);
			}
			if (idx < 0 || !pool[idx].present || pool[idx].freed) {
				int dead = (int)((r >> 20) % N);
				int ret = rhashtable_remove_fast(
					&ht, &pool[dead].rhl.rhead, g_par);
				fold((uint64_t)(int64_t)ret);
				if (!ret)
					pool[dead].present = 0;
				fold(NILX);
			} else {
				fold((uint64_t)(int64_t)rhashtable_remove_fast(
					&ht, &pool[idx].rhl.rhead, g_par));
				fold(idx);
				pool[idx].present = 0;
			}
		} else if (t < 65) {	/* replace_fast */
			int c = count_present();
			int ni = free_slot();
			if (!c || ni < 0) {
				fold(NILX);
			} else {
				int v = nth_present((r >> 10) % c);
				int ret;
				pool[ni].meta = xs64();
				pool[ni].key = (r & 0x800)
					? (xs64() % KEYSPACE)
					: pool[v].key;
				pool[ni].tag = xs64() % 1000;
				ret = rhashtable_replace_fast(
					&ht, &pool[v].rhl.rhead,
					&pool[ni].rhl.rhead, g_par);
				fold((uint64_t)(int64_t)ret);
				fold(v);
				fold(ni);
				if (!ret) {
					pool[v].present = 0;
					pool[ni].present = 1;
				}
			}
		} else if (t < 72) {	/* lookups */
			uint64_t k = xs64() % KEYSPACE;
			void *o;
			switch ((r >> 8) % 3) {
			case 0:
				o = rhashtable_lookup_fast(&ht, &k, g_par);
				break;
			case 1:
				o = rhashtable_lookup(&ht, &k, g_par);
				break;
			default:
				o = rhashtable_lookup_likely(&ht, &k, g_par);
				break;
			}
			fold(canon(o));
			fold(tidx(o));
		} else if (t < 77) {	/* next_key iteration, bounded */
			void *o;
			const void *pk = NULL;
			int steps = 0, c = count_present();
			if (c && (r & 0x400)) {
				int s = nth_present((r >> 12) % c);
				pk = &pool[s].key;
				fold(s);
			} else
				fold(NILX);
			do {
				o = rhashtable_next_key(&ht, pk);
				if (IS_ERR(o)) {
					fold((uint64_t)PTR_ERR(o));
					break;
				}
				fold(canon(o));
				if (!o)
					break;
				pk = &((struct tobj *)o)->key;
			} while (++steps < 12);
			fold(steps);
		} else if (t < 87) {	/* walks */
			op_walk(&ht, r, 0);
		} else if (t < 93) {	/* work drain / forced resize */
			drain_work(&ht);
		} else if (t < 96) {	/* destroy + reinit */
			int k;
			for (k = 0; k < NI; k++) {
				if (walk_on[k])
					rhashtable_walk_stop(&iters[k]);
				if (iter_entered[k])
					rhashtable_walk_exit(&iters[k]);
				iter_entered[k] = walk_on[k] = 0;
			}
			fold(0xde5700);
			rhashtable_free_and_destroy(&ht, free_cb, NULL);
			ht_live = 0;
			fold(0xde5701);
		} else {		/* lookups of absent keys + get paths */
			uint64_t k = KEYSPACE + (xs64() % 64);
			void *o = rhashtable_lookup_fast(&ht, &k, g_par);
			fold(canon(o));
			fold(atomic_read(&ht.nelems));
		}

		fold(atomic_read(&ht.nelems));
		if (i % 33 == 32)
			dump_ht(&ht);
	}

	fold(0xe9d0000);
	dump_ht(&ht);
	fold(0xe9d1111);
	rhashtable_free_and_destroy(&ht, free_cb, NULL);
	ht_live = 0;
	return g_h;
}

/* ================= wrap_rhlt_seq: rhltable op mix ================= */

static void rhl_fold_lookup(struct rhltable *t, uint64_t k)
{
	struct rhlist_head *l, *pos;
	struct tobj *to;
	int c = 0;
	rcu_read_lock();
	l = rhltable_lookup(t, &k, g_par);
	fold(canon(l));
	if (l) {
		rhl_for_each_entry_rcu(to, pos, l, rhl) {
			fold(canon(pos));
			fold(to->key);
			if (++c == 8)
				break;
		}
	}
	fold(c);
	rcu_read_unlock();
}

uint64_t wrap_rhlt_seq(uint64_t seed, int64_t nops_i, int64_t fail_i)
{
	int nops = 40 + (int)(iabs(nops_i) % 480);
	int i;

	reset_all(seed, fail_i);
	g_par = mk_params(xs64(), 1);
	fold((uint64_t)(int64_t)rhltable_init(&hlt, &g_par));
	hlt_live = 1;
	{
		struct bucket_table *t = rcu_dereference_raw(hlt.ht.tbl);
		fold(t ? t->size : 0);
	}

	for (i = 0; i < nops; i++) {
		uint64_t r = xs64();
		int t = (int)(r % 100);

		if (!hlt_live) {
			g_par = mk_params(xs64(), 1);
			fold((uint64_t)(int64_t)rhltable_init(&hlt, &g_par));
			hlt_live = 1;
			continue;
		}

		if (t < 40) {		/* rhltable_insert[_key]: dup keys ok */
			uint64_t k = pick_key();
			int idx = prep_obj(k);
			if (idx < 0) {
				fold(NILX);
			} else {
				int ret;
				if (r & 0x400)
					ret = rhltable_insert(&hlt,
						&pool[idx].rhl, g_par);
				else
					ret = rhltable_insert_key(&hlt,
						&pool[idx].key,
						&pool[idx].rhl, g_par);
				pool[idx].present = !ret;
				fold((uint64_t)(int64_t)ret);
				fold(idx);
			}
		} else if (t < 55) {	/* rhltable_remove */
			int c = count_present();
			int idx;
			if (c && (r & 0x300))
				idx = nth_present((r >> 10) % c);
			else
				idx = (int)((r >> 10) % N);
			if (idx < 0 || !pool[idx].present || pool[idx].freed) {
				int dead = (int)((r >> 20) % N);
				fold((uint64_t)(int64_t)rhltable_remove(
					&hlt, &pool[dead].rhl, g_par));
				fold(NILX);
			} else {
				fold((uint64_t)(int64_t)rhltable_remove(
					&hlt, &pool[idx].rhl, g_par));
				fold(idx);
				pool[idx].present = 0;
			}
		} else if (t < 70) {	/* rhltable_lookup + list walk */
			rhl_fold_lookup(&hlt, xs64() % KEYSPACE);
		} else if (t < 77) {	/* walk ops */
			op_walk(&hlt.ht, r, 1);
		} else if (t < 83) {	/* work drain */
			drain_work(&hlt.ht);
		} else if (t < 88) {	/* destroy + reinit */
			int k;
			for (k = 0; k < NI; k++) {
				if (walk_on[k])
					rhashtable_walk_stop(&iters[k]);
				if (iter_entered[k])
					rhashtable_walk_exit(&iters[k]);
				iter_entered[k] = walk_on[k] = 0;
			}
			fold(0xde5702);
			rhltable_free_and_destroy(&hlt, free_cb, NULL);
			hlt_live = 0;
			fold(0xde5703);
		} else {		/* plain lookups, mixed hit/miss */
			uint64_t k = (r & 0x800)
				? pick_key() : xs64() % KEYSPACE;
			rhl_fold_lookup(&hlt, k);
			{
				struct rhlist_head *l2;
				rcu_read_lock();
				l2 = rhltable_lookup_likely(&hlt, &k, g_par);
				fold(canon(l2));
				rcu_read_unlock();
			}
		}

		fold(atomic_read(&hlt.ht.nelems));
		if (i % 33 == 32)
			dump_ht(&hlt.ht);
	}

	fold(0xe9d2222);
	dump_ht(&hlt.ht);
	rhltable_free_and_destroy(&hlt, free_cb, NULL);
	hlt_live = 0;
	return g_h;
}

/* ============ wrap_rht_walk: dedicated iterator coverage ============ */

uint64_t wrap_rht_walk(uint64_t seed, int64_t n_i, int64_t fail_i)
{
	int n = 4 + (int)(iabs(n_i) % 60);
	int i, j;

	reset_all(seed, fail_i);
	g_par = mk_params(xs64(), 0);
	g_par.obj_hashfn = NULL;
	g_par.obj_cmpfn = NULL;
	fold((uint64_t)(int64_t)rhashtable_init(&ht, &g_par));
	ht_live = 1;

	for (i = 0; i < n; i++) {
		int idx = prep_obj(xs64() % KEYSPACE);
		int ret;
		if (idx < 0)
			break;
		ret = rhashtable_lookup_insert_fast(&ht, &pool[idx].rhl.rhead,
						  g_par);
		pool[idx].present = !ret;
		fold((uint64_t)(int64_t)ret);
	}
	drain_work(&ht);
	dump_ht(&ht);

	/* a scripted walk: enter, start, next/peek mix with mid-walk
	 * mutations, stop/start restarts, exit. */
	rhashtable_walk_enter(&ht, &iters[0]);
	iter_entered[0] = 1;
	for (j = 0; j < 48; j++) {
		uint64_t r = xs64();
		switch (r % 6) {
		case 0: {
			void *o;
			if (!walk_on[0]) {
				fold((uint64_t)(int64_t)
					rhashtable_walk_start_check(&iters[0]));
				walk_on[0] = 1;
				break;
			}
			o = rhashtable_walk_next(&iters[0]);
			fold(IS_ERR(o) ? (uint64_t)PTR_ERR(o) : canon(o));
			break;
		}
		case 1:
			if (walk_on[0]) {
				void *o = rhashtable_walk_peek(&iters[0]);
				fold(IS_ERR(o) ? (uint64_t)PTR_ERR(o)
					       : canon(o));
			} else
				fold(NILX);
			break;
		case 2:
			if (walk_on[0]) {
				rhashtable_walk_stop(&iters[0]);
				walk_on[0] = 0;
			}
			fold(0x57);
			break;
		case 3: {	/* mutate: insert or remove */
			int c = count_present();
			if ((r & 0x400) || !c) {
				int idx = prep_obj(xs64() % KEYSPACE);
				if (idx >= 0) {
					int ret = rhashtable_lookup_insert_fast(
						&ht, &pool[idx].rhl.rhead,
						g_par);
					pool[idx].present = !ret;
					fold((uint64_t)(int64_t)ret);
				}
			} else {
				int v = nth_present((r >> 12) % c);
				fold((uint64_t)(int64_t)
					rhashtable_remove_fast(
						&ht, &pool[v].rhl.rhead,
						g_par));
				pool[v].present = 0;
				fold(v);
			}
			break;
		}
		case 4:
			drain_work(&ht);
			break;
		default:
			dump_iters();
			break;
		}
	}
	if (walk_on[0])
		rhashtable_walk_stop(&iters[0]);
	rhashtable_walk_exit(&iters[0]);
	iter_entered[0] = 0;
	dump_ht(&ht);
	rhashtable_free_and_destroy(&ht, free_cb, NULL);
	return g_h;
}

/* ===== wrap_rht_elastic: collisions, slow path, nested tables ===== */

uint64_t wrap_rht_elastic(uint64_t seed, int64_t n_i, int64_t fail_i)
{
	int n = 4 + (int)(iabs(n_i) % 920);
	int i, mode;

	reset_all(seed, fail_i);
	mode = (int)(xs64() % 3);
	memset(&g_par, 0, sizeof g_par);
	g_par.key_len = 8;
	g_par.key_offset = KEY_OFF;
	g_par.head_offset = HEAD_OFF;
	g_par.min_size = 4;
	if (mode == 0) {	/* colliding obj hash, secure elasticity */
		g_par.obj_hashfn = obj_hash_bad;
		g_par.obj_cmpfn = obj_cmp_eq;
		g_par.max_size = 2048;
	} else if (mode == 1) {	/* insecure: chains grow unbounded */
		g_par.obj_hashfn = obj_hash_bad;
		g_par.obj_cmpfn = obj_cmp_eq;
		g_par.insecure_elasticity = 1;
	} else {		/* tiny max_size: growth capped */
		g_par.max_size = 16;
	}
	fold(mode);
	fold((uint64_t)(int64_t)rhashtable_init(&ht, &g_par));
	ht_live = 1;
	{
		struct bucket_table *t = rcu_dereference_raw(ht.tbl);
		fold(t ? t->size : 0);
	}

	for (i = 0; i < n; i++) {
		uint64_t r = xs64();
		int idx = prep_obj(r % KEYSPACE);
		int ret;
		if (idx < 0) {
			fold(NILX);
			break;
		}
		if (g_par.obj_hashfn)
			ret = rhashtable_lookup_insert_key(
				&ht, &pool[idx].key, &pool[idx].rhl.rhead,
				g_par);
		else
			ret = rhashtable_lookup_insert_fast(
				&ht, &pool[idx].rhl.rhead, g_par);
		pool[idx].present = !ret;
		fold((uint64_t)(int64_t)ret);
		fold(atomic_read(&ht.nelems));
		if (i % 9 == 8)
			drain_work(&ht);
		if (i % 17 == 16) {
			int c = count_present();
			if (c) {
				int v = nth_present(r % c);
				fold((uint64_t)(int64_t)
					rhashtable_remove_fast(
						&ht, &pool[v].rhl.rhead,
						g_par));
				pool[v].present = 0;
			}
		}
	}
	drain_work(&ht);
	dump_ht(&ht);
	fold(0xe9d3333);
	rhashtable_free_and_destroy(&ht, free_cb, NULL);
	return g_h;
}

/* ============ wrap_bkl: lib/bucket_locks.c coverage ============ */

uint64_t wrap_bkl(uint64_t seed, int64_t n_i, int64_t fail_i)
{
	int n = 1 + (int)(iabs(n_i) % 8);
	int i;

	reset_all(seed, fail_i);
	(void)pool;

	for (i = 0; i < n; i++) {
		uint64_t r = xs64();
		size_t max_size = (size_t)((r >> 8) % 1025);
		unsigned int cpu_mult = (unsigned int)((r >> 24) % 5);
		gfp_t gfp = (r & 1) ? GFP_KERNEL : GFP_ATOMIC;
		spinlock_t *locks = NULL;
		unsigned int mask = 0;
		int ret = alloc_bucket_spinlocks(&locks, &mask, max_size,
						 cpu_mult, gfp);
		fold((uint64_t)(int64_t)ret);
		fold(mask);
		if (!ret) {
			unsigned int s = mask + 1;
			int k = (int)((r >> 32) % 3) + 1;
			while (k-- && s) {
				unsigned int b = (unsigned int)(xs64() % s);
				spin_lock(&locks[b]);
				spin_unlock(&locks[b]);
			}
			fold(canon(locks));
			free_bucket_spinlocks(locks);
		}
		fold(g_alloc_ord);
	}
	return g_h;
}
