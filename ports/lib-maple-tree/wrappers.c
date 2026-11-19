/* Scalar wrappers around lib/maple_tree.c and the include/linux/maple_tree.h
 * inline layer, so the standard scalar bridge can drive them.
 *
 * Address-space model: the maple tree stores real C pointers, so pointer
 * identity must be foldable.  struct maple_node lives in a static pool of
 * NP objects reached through the kmem_cache shim (the unit boundary is
 * lib/maple_tree.c's maple_node_cache; see linux/slab.h).  Slab sheaves
 * (mas->sheaf bulk allocation) live in a second pool of NSP objects.
 * User entries are pointers into an aligned u64 entry pool (|1 or |3
 * tagged variants allowed: only low-bit '10' values are reserved).
 *
 * efold() maps every pointer to ONE canonical u64 (also the exact
 * encoding the Rust port uses as its pointer values):
 *   NULL                     -> 0
 *   entry pool &epool[i]     -> EBASE + i*16 (+ tag for tagged pointers)
 *   node pool &node_pool[i]  -> NBASE + i*256 (+ enode low bits)
 *   &mt / &mt2               -> MT_CANON / MT2_CANON (+ low bits)
 *   sheaf_pool[i]            -> SHBASE + i*256
 *   MA_ERROR(err) sentinels  -> the integer itself (already canonical)
 *   raw values (pivots, gaps, min/max) -> unchanged (they live below
 *   EBASE/NBASE so they can never alias a canonical pointer: generated
 *   indices and pivots stay under 2^46 or near ULONG_MAX)
 *
 * Allocation failure injection: a per-wrapper u64 fail_mask nominates
 * which alloc calls (1..64, counted in alloc_ctr) return NULL — node
 * allocs, sheaf structs and sheaf member fills all consume ordinals,
 * driving mas_alloc_nodes/mas_nomem/mas_nomem_nofail/prealloc failures.
 *
 * Every wrapper returns one order-sensitive FNV-1a-style checksum folded
 * over each op's results (entries, errnos, mas state) and a periodic
 * FULL TREE DUMP: per live pool node all 32 u64 words canonicalised,
 * plus ma_flags, ma_root, live-sheaf contents.  Raw addresses never fold.
 */
#include <stdint.h>
#include <stddef.h>
#include <string.h>
#include <linux/types.h>
#include <linux/lockdep.h>
#include <linux/maple_tree.h>
#include <linux/slab.h>
#include <linux/errno.h>
#include <linux/gfp.h>

#define NP 448		/* maple_node pool slots (256B each) */
#define EP 96		/* user entry objects */
#define NSP 12		/* slab_sheaf pool */
#define NILX 0xffffu	/* "no such index" fold sentinel */

/* canonical encodings shared with port.rs */
#define NBASE 0x0010000000000000ULL	/* node canonical base, stride 0x100 */
#define EBASE 0x0020000000000000ULL	/* entry canonical base, stride 16 */
#define MT_CANON  0x0080000000000000ULL	/* &mt canonical */
#define MT2_CANON 0x0080000000001000ULL	/* &mt2 canonical */
#define SHBASE  0x00A0000000000000ULL	/* sheaf canonical base */

struct slab_sheaf {
	unsigned int size;
	struct maple_node *objs[32];
};

static struct maple_node node_pool[NP] __attribute__((aligned(256)));
static uint8_t node_used[NP];
static struct slab_sheaf sheaf_pool[NSP];
static uint8_t sheaf_used[NSP];
static uint64_t epool[EP] __attribute__((aligned(64)));
static struct maple_tree mt, mt2;
struct lockdep_map rcu_callback_map;

unsigned long kshim_warn_hits;

static struct kmem_cache the_cache;
_Static_assert(sizeof(struct maple_node) == 256, "maple_node is 256B");

/* ---- deterministic allocator (slab boundary, see linux/slab.h) ---- */

static unsigned long alloc_ctr;
static uint64_t fail_mask;

/* one alloc ordinal per call; NULL when the seeded mask says so or the
 * pool is exhausted.  No memset: slab does not zero, stale contents are
 * deterministic anyway. */
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

static struct slab_sheaf *sheaf_alloc(void)
{
	int i;
	alloc_ctr++;
	if (alloc_ctr <= 64 && (fail_mask >> (alloc_ctr - 1)) & 1)
		return NULL;
	for (i = 0; i < NSP; i++)
		if (!sheaf_used[i]) {
			sheaf_used[i] = 1;
			return &sheaf_pool[i];
		}
	return NULL;
}

static void sheaf_free(struct slab_sheaf *sh)
{
	if (!sh)
		return;
	sheaf_used[sh - sheaf_pool] = 0;
}

/* ---- kmem_cache API the unit calls ---- */

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

/* prefill: sheaf struct alloc consumes an ordinal, then `size` node
 * allocs each consume one; failure frees the partial contents and the
 * sheaf, returning NULL (alloc-error path in mas_alloc_nodes). */
struct slab_sheaf *kmem_cache_prefill_sheaf(struct kmem_cache *s,
					    gfp_t gfp, unsigned int size)
{
	struct slab_sheaf *sh;
	unsigned int i;

	(void)s;
	sh = sheaf_alloc();
	if (!sh)
		return NULL;
	sh->size = 0;
	for (i = 0; i < size && i < 32; i++) {
		struct maple_node *n = node_alloc(gfp);
		if (!n) {
			unsigned int j;
			for (j = 0; j < i; j++)
				node_free(sh->objs[j]);
			sheaf_free(sh);
			return NULL;
		}
		sh->objs[i] = n;
	}
	sh->size = i;
	return sh;
}

/* refill to `size` objects: each additional node alloc consumes an
 * ordinal; a failure keeps the nodes already added and returns -ENOMEM
 * (the sheaf contents are real allocations either way). */
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
	sheaf_free(sheaf);
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

static int node_idx(const struct maple_node *n)
{
	return (int)(n - node_pool);
}

static uint64_t ncanon(const struct maple_node *n)
{
	return n ? NBASE + (uint64_t)node_idx(n) * 256 : 0;
}

static uint64_t scanon(const struct slab_sheaf *sh)
{
	return sh ? SHBASE + (uint64_t)(sh - sheaf_pool) * 256 : 0;
}

static uint64_t efold(const void *e)
{
	uintptr_t p = (uintptr_t)e;
	uintptr_t np = (uintptr_t)node_pool;
	if (!p)
		return 0;
	if (p >= np && p < np + sizeof(node_pool))
		return NBASE + ((p - np) & ~0xFFUL) + (p & 0xFF);
	if (p >= (uintptr_t)epool &&
	    p < (uintptr_t)epool + sizeof(epool))
		/* preserve low tag bits in the canonical encoding */
		return EBASE + ((p - (uintptr_t)epool) & ~7ULL) * 2 +
			(p & 3);
	/* Tree-struct pointers fold as base + offset within the struct.
	 * Using p & 0xFF would leak the .bss layout's absolute low byte,
	 * which varies between builds and is not canonical. */
	if (p >= (uintptr_t)&mt && p < (uintptr_t)&mt + sizeof(mt))
		return MT_CANON | (p - (uintptr_t)&mt);
	if (p >= (uintptr_t)&mt2 && p < (uintptr_t)&mt2 + sizeof(mt2))
		return MT2_CANON | (p - (uintptr_t)&mt2);
	if (p >= (uintptr_t)sheaf_pool &&
	    p < (uintptr_t)sheaf_pool + sizeof(sheaf_pool))
		return SHBASE + ((p - (uintptr_t)sheaf_pool) & ~0xFFUL) +
			(p & 0xFF);
	return p;	/* MA_ERROR/values are already canonical */
}

/* ---- generators ---- */

/* index space: dense small ranges (leaf packing), node-crossing runs,
 * far-apart regions (multi-level trees) and the ULONG_MAX corner.
 * All values stay below 2^46 or within a few KB of ULONG_MAX so they
 * can never alias a canonical pointer encoding. */
static unsigned long gen_idx(uint64_t r)
{
	switch (r % 16) {
	case 0: case 1: case 2:
		return (r >> 8) % 64;
	case 3: case 4:
		return (r >> 8) % 1024;
	case 5:
		return 60 + (r >> 8) % 9;		/* ~64 boundary */
	case 6:
		return (r >> 8) % 65536;
	case 7:
		return 4090 + (r >> 8) % 12;
	case 8:
		return ((r >> 8) % (1UL << 16)) << 4;
	case 9:
		return (r >> 8) % (1UL << 20);
	case 10:
		return ((r >> 8) % (1UL << 8)) << 12;
	case 11:
		return ((r >> 8) % (1UL << 12)) << 16;
	case 12:
		return ((r >> 8) % (1UL << 10)) << 24;
	case 13:
		return (1UL << 32) + (r >> 8) % (1UL << 20);
	case 14:
		return ULONG_MAX - (r >> 8) % 2048;
	default:
		return (ULONG_MAX >> 1) + (r >> 8) % 256;
	}
}

/* range endpoints: {start, last} with last >= start.  Small spans for
 * in-node stores, medium spans crossing leaf bounds, huge spans that
 * trigger spanning rebalances. */
static void gen_range(uint64_t r, unsigned long *start, unsigned long *last)
{
	unsigned long s = gen_idx(r);
	unsigned long len;
	switch ((r >> 3) % 12) {
	case 0: case 1: case 2:
		len = (r >> 11) % 4;
		break;
	case 3: case 4:
		len = (r >> 11) % 20;
		break;
	case 5:
		len = 14 + (r >> 11) % 4;
		break;
	case 6: case 7:
		len = (r >> 11) % 200;
		break;
	case 8:
		len = (r >> 11) % 4096;
		break;
	case 9:
		len = (r >> 11) % (1UL << 16);
		break;
	case 10:
		len = (r >> 11) % (1UL << 20);
		break;
	default:
		len = ULONG_MAX / 4 - (r >> 11) % 64;
		if (s > ULONG_MAX / 2)
			len = (r >> 11) % 64;
		break;
	}
	if (len > ULONG_MAX - s)
		len = ULONG_MAX - s;
	*start = s;
	*last = s + len;
}

static void *gen_entry(uint64_t r)
{
	switch (r % 12) {
	case 0: case 1:
		return NULL;
	case 2: case 3: case 4: case 5: case 6: case 7:
		return &epool[(r >> 8) % EP];
	case 8:
		return (void *)((unsigned long)&epool[(r >> 8) % EP] | 1);
	case 9:
		return (void *)((unsigned long)&epool[(r >> 8) % EP] | 3);
	default:
		return &epool[(r >> 8) % EP];
	}
}

static void *gen_entry_nonnull(uint64_t r)
{
	void *e = gen_entry(r);
	if (!e)
		e = &epool[(r >> 16) % EP];
	return e;
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

/*
 * Semantic node dump.  Upstream builds nodes in a stack "reuse" buffer
 * (mas_wr_node_store) whose dead bytes carry uninitialised stack data
 * into live nodes via whole-struct memcpy.  Those bytes are never read
 * by the tree code, but folding raw node memory would mix ASLR/stack
 * garbage into the checksum and make it non-deterministic.  So each
 * node's type is discovered by walking the tree from the root enode and
 * only the semantic words of that type's layout are folded:
 *
 *   leaf64       : w0 (parent) + pivots w1..w15 + slot0 w16, then w31
 *                  (see dump_last_word: last slot pointer or meta.end +
 *                  dead bytes).  Slots 1..14 (w17..w30) are not folded:
 *                  past-data_end slots may hold stale reuse-buffer bytes.
 *   dense        : w0 + slots w1..w30, then w31 (same union tail rule).
 *   range64      : w0 + pivots w1..w15 + slots w16..w30, then w31 (same
 *                  slot[15]-or-meta union tail as leaf64).
 *   arange64     : w0 + pivots w1..w9 + slots w10..w19 + gaps w20..w29
 *                  (only gaps 0..meta.end are semantic), then meta in
 *                  w30 low bytes; w30 high bytes and w31 are padding.
 *   other/unlinked: index and a 0xff sentinel only.  Nodes held in a
 *                  sheaf or under copy construction are scratch space.
 *
 * meta.end lives in the low byte of the last used struct word: mr64.meta
 * is at offset 248 (w31) while ma64.meta is at offset 240 (w30), matching
 * ma_meta().
 */

static uint8_t dump_ntype[NP];

static int dump_t_slots(enum maple_type t)
{
	switch (t) {
	case maple_range_64:	return 15;
	case maple_arange_64:	return 10;
	case maple_leaf_64:	return 16;
	case maple_dense:	return 30;
	default:		return 0;
	}
}

static int dump_t_slot_base(enum maple_type t)
{
	switch (t) {
	case maple_range_64:	return 16;
	case maple_arange_64:	return 10;
	case maple_leaf_64:
	case maple_dense:	return 1;
	default:		return 0;
	}
}

static void dump_walk(unsigned long e)
{
	uintptr_t base;
	enum maple_type t;
	int idx, i, end, sb, ns;
	const uint64_t *words;

	if (!e)
		return;
	base = e & ~0xffUL;
	if (base < (uintptr_t)node_pool ||
	    base >= (uintptr_t)node_pool + sizeof(node_pool))
		return;				/* direct entry / NULL */
	idx = (int)((base - (uintptr_t)node_pool) >> 8);
	if (idx < 0 || idx >= NP || dump_ntype[idx] != 0xff)
		return;
	t = (enum maple_type)((e >> 3) & 0x0f);
	dump_ntype[idx] = t;
	if (t != maple_range_64 && t != maple_arange_64)
		return;				/* leaf: no children */
	words = (const uint64_t *)&node_pool[idx];
	/* meta.end: mr64 types keep it in w31 low byte; arange64 in w30. */
	end = (t == maple_arange_64 ? words[30] : words[31]) & 0xff;
	sb = dump_t_slot_base(t);
	ns = dump_t_slots(t);
	for (i = 0; i <= end && i < ns; i++)
		dump_walk(words[sb + i]);
}

/*
 * meta.end is consulted by ma_data_end() only when pivot[14]==0 (word
 * 15 == 0 for both leaf64 and range64 layouts), and unconditionally for
 * arange64.  Leaf64 nodes under MT_FLAGS_ALLOC_RANGE additionally use
 * meta.end/meta.gap via mas_leaf_max_gap().  Otherwise the two metadata
 * bytes are advisory and may hold stale copy scratch — fold 0 instead.
 */
static int dump_meta_live(const struct maple_tree *t, enum maple_type mt,
			  const uint64_t *words)
{
	if (mt == maple_arange_64)
		return 1;
	if (mt == maple_dense)
		return 0;
	if (mt == maple_leaf_64 && (t->ma_flags & MT_FLAGS_ALLOC_RANGE))
		return 1;
	return words[15] == 0;
}

/*
 * Word 31 of the mr64-based layouts (dense/leaf64/range64) is a union
 * tail: it either holds the last slot pointer (dense slot[30],
 * leaf64/range64 slot[15]) or, when that slot is unused, the metadata
 * {end,gap} bytes plus 6 dead bytes.  A live pointer there must be
 * folded canonically — folding raw low bits would leak ASLR-dependent
 * address bits and make the checksum nondeterministic.  When it is
 * metadata, only meta.end (byte 0) is ever read by upstream for these
 * types (ma_meta_gap() resolves to the arange64 offset at byte 241), so
 * the gap byte and upper padding are dead and must not be folded.
 */
static void dump_last_word(const struct maple_tree *t, enum maple_type nt,
			   const uint64_t *words)
{
	uint64_t w31 = words[31];
	uint64_t e = efold((const void *)w31);

	if (e != w31) {
		fold(e);			/* last slot holds a live pointer */
		return;
	}
	fold(dump_meta_live(t, nt, words) ? w31 & 0xff : 0);
}

static void dump_tree(struct maple_tree *t)
{
	int i, w, nused = 0;
	uint64_t *words;

	fold(t->ma_flags);
	fold(efold(t->ma_root));
	for (i = 0; i < NP; i++)
		nused += node_used[i];
	fold(nused);
	memset(dump_ntype, 0xff, sizeof(dump_ntype));
	dump_walk((unsigned long)t->ma_root);
	for (i = 0; i < NP; i++) {
		enum maple_type nt;
		if (!node_used[i])
			continue;
		fold(i);
		nt = dump_ntype[i];
		fold(nt);
		words = (uint64_t *)&node_pool[i];
		switch (nt) {
		case maple_dense:
			for (w = 0; w <= 30; w++)
				fold(efold((void *)words[w]));
			dump_last_word(t, nt, words);
			break;
		case maple_leaf_64:
			for (w = 0; w <= 16; w++)
				fold(efold((void *)words[w]));
			dump_last_word(t, nt, words);
			break;
		case maple_range_64:
			for (w = 0; w <= 30; w++)
				fold(efold((void *)words[w]));
			dump_last_word(t, nt, words);
			break;
		case maple_arange_64: {
			/* ma64: pivots w1..9, slots w10..19, gaps w20..29,
			 * meta at w30 low bytes; w30 high + w31 are padding. */
			int end = words[30] & 0xff;
			for (w = 0; w <= 19; w++)
				fold(efold((void *)words[w]));
			for (w = 20; w <= 29; w++)
				fold(w - 20 <= end ?
				     efold((void *)words[w]) : 0);
			fold(words[30] & 0xffff);
			break;
		}
		default:
			/* unreachable: scratch/sheaf/dead node */
			break;
		}
	}
	for (i = 0; i < NSP; i++) {
		unsigned int j;
		if (!sheaf_used[i])
			continue;
		fold(i | 0x4000);
		fold(sheaf_pool[i].size);
		for (j = 0; j < sheaf_pool[i].size; j++)
			fold(efold(sheaf_pool[i].objs[j]));
	}
	fold(NILX);
}

static void dump_both(void)
{
	dump_tree(&mt);
	dump_tree(&mt2);
}

static void fold_mas(const struct ma_state *mas)
{
	fold(mas->index);
	fold(mas->last);
	fold(mas->min);
	fold(mas->max);
	fold((uint64_t)mas->status);
	fold(mas->depth);
	fold(mas->offset);
	fold(mas->end);
	fold(mas->mas_flags);
	fold(mas->node_request);
	fold((uint64_t)mas->store_type);
	fold(efold(mas->node));
	fold(ncanon(mas->alloc));
	fold(scanon(mas->sheaf));
}

static void reset_all(unsigned int flags)
{
	int i;
	for (i = 0; i < NP; i++) {
		memset(&node_pool[i], 0, sizeof node_pool[i]);
		node_used[i] = 0;
	}
	for (i = 0; i < NSP; i++)
		sheaf_used[i] = 0;
	memset(epool, 0, sizeof epool);
	for (i = 0; i < EP; i++)
		epool[i] = 0x1000 + i;
	maple_tree_init();
	mt_init_flags(&mt, flags);
	mt_init_flags(&mt2, flags);
	alloc_ctr = 0;
	kshim_warn_hits = 0;
	g_h = 0x9e3779b97f4a7c15ULL;
}

/* ================= op helpers (shared by the drivers) ================= */

static const unsigned int flag_tab[6] = {
	0,
	MT_FLAGS_ALLOC_RANGE,
	MT_FLAGS_USE_RCU,
	MT_FLAGS_USE_RCU | MT_FLAGS_ALLOC_RANGE,
	MT_FLAGS_LOCK_EXTERN,
	MT_FLAGS_USE_RCU | MT_FLAGS_ALLOC_RANGE | MT_FLAGS_LOCK_EXTERN,
};

/* recently stored ranges: reuse picks them so overwrite/split paths hit
 * live entries instead of always growing the tree. */
struct hot_s {
	unsigned long start;
	unsigned long last;
	unsigned char live;
};
static struct hot_s hot[16];

static void hot_note(unsigned long s, unsigned long l)
{
	int i;
	for (i = 0; i < 16; i++)
		if (hot[i].live && hot[i].start == s && hot[i].last == l)
			return;
	for (i = 0; i < 16; i++)
		if (!hot[i].live) {
			hot[i].start = s;
			hot[i].last = l;
			hot[i].live = 1;
			return;
		}
	hot[15].start = s;
	hot[15].last = l;
	hot[15].live = 1;
}

static void hot_kill(unsigned long s, unsigned long l)
{
	int i;
	for (i = 0; i < 16; i++) {
		if (!hot[i].live)
			continue;
		if (hot[i].start <= l && s <= hot[i].last)
			hot[i].live = 0;
	}
}

static int hot_pick(uint64_t r, unsigned long *s, unsigned long *l)
{
	int nth = (int)(r % 3), i;
	for (i = 0; i < 16; i++)
		if (hot[i].live && nth-- == 0) {
			*s = hot[i].start;
			*l = hot[i].last;
			return 1;
		}
	for (i = 0; i < 16; i++)
		if (hot[i].live) {
			*s = hot[i].start;
			*l = hot[i].last;
			return 1;
		}
	return 0;
}

/* store [s,l] = e via mas_store_gfp (single-index or range).  The gfp
 * path inside mas_store_gfp already retries through mas_nomem. */
static void op_store(uint64_t r)
{
	unsigned long s, l;
	void *e = gen_entry(r >> 20);
	MA_STATE(mas, &mt, 0, 0);
	int ret;

	if ((r >> 6 & 1) && hot_pick(r >> 10, &s, &l)) {
		/* overwrite/split a live range: sometimes shrink it */
		if (r & 0x40) {
			l = s + (l - s) / 2;
		} else if (r & 0x80) {
			l = s;
		}
	} else {
		gen_range(r, &s, &l);
	}
	mas_set_range(&mas, s, l);
	ret = mas_store_gfp(&mas, e, gen_gfp(r >> 40));
	fold((uint64_t)(int64_t)ret);
	fold_mas(&mas);
	if (e)
		hot_note(s, l);
	else
		hot_kill(s, l);
}

static void op_store_range(uint64_t r)
{
	unsigned long s, l;
	void *e = gen_entry(r >> 24);
	int ret;

	gen_range(r, &s, &l);
	if (r & 0x2000) {
		unsigned long t = s;
		s = l; l = t;			/* s > l: -EINVAL path */
	}
	ret = mtree_store_range(&mt, s, l, e, gen_gfp(r >> 44));
	fold((uint64_t)(int64_t)ret);
	if (e)
		hot_note(s, l);
	else if (s <= l)
		hot_kill(s, l);
}

static void op_insert(uint64_t r)
{
	unsigned long s, l;
	void *e = gen_entry_nonnull(r >> 20);
	int ret;

	if ((r >> 5 & 1) && hot_pick(r >> 9, &s, &l)) {
		/* insert over a live range -> -EEXIST coverage */
	} else {
		gen_range(r, &s, &l);
	}
	if (r & 0x400) {
		if (s == l || r & 0x800)
			ret = mtree_insert(&mt, s, e, gen_gfp(r >> 30));
		else
			ret = mtree_insert_range(&mt, s, l, e,
						 gen_gfp(r >> 30));
	} else {
		ret = mtree_insert_range(&mt, s, l, e, gen_gfp(r >> 30));
	}
	fold((uint64_t)(int64_t)ret);
}

static void op_load(uint64_t r)
{
	unsigned long idx;
	void *e;

	if ((r >> 4 & 1) && hot_pick(r >> 9, &idx, &idx)) {
		if (r & 0x200)
			idx += (r >> 20) % 8;	/* land inside a range */
	} else {
		idx = gen_idx(r);
	}
	if (r & 0x8000) {
		MA_STATE(mas, &mt, idx, idx);
		e = mas_walk(&mas);
		fold(efold(e));
		fold_mas(&mas);
	} else {
		e = mtree_load(&mt, idx);
		fold(efold(e));
	}
}

static void op_erase(uint64_t r)
{
	unsigned long idx;
	void *e;

	if ((r >> 4 & 1) && hot_pick(r >> 9, &idx, &idx)) {
		if (r & 0x200)
			idx += (r >> 20) % 8;
	} else {
		idx = gen_idx(r);
	}
	if (r & 0x4000) {
		MA_STATE(mas, &mt, idx, idx);
		e = mas_erase(&mas);
		fold(efold(e));
		fold_mas(&mas);
	} else {
		e = mtree_erase(&mt, idx);
		fold(efold(e));
	}
	hot_kill(idx, idx);
}

/* a cursor walk: set the state, then a mix of mas_next/prev/find/walk
 * steps folding every result and its [index,last] range */
static void op_walk(uint64_t r)
{
	unsigned long idx;
	MA_STATE(mas, &mt, 0, 0);
	void *e;
	int k, steps = 4 + (int)((r >> 33) % 8);

	if ((r >> 4 & 1) && hot_pick(r >> 9, &idx, &idx)) {
		;
	} else {
		idx = gen_idx(r);
	}
	mas_set(&mas, idx);
	for (k = 0; k < steps; k++) {
		uint64_t q = xs64();
		switch (q % 8) {
		case 0:
			e = mas_walk(&mas);
			break;
		case 1:
			e = mas_next(&mas, (q & 0x80) ? ULONG_MAX :
					 gen_idx(q >> 12));
			break;
		case 2:
			e = mas_prev(&mas, (q & 0x80) ? 0 :
					 gen_idx(q >> 12) % 256);
			break;
		case 3:
			e = mas_find(&mas, (q & 0x80) ? ULONG_MAX :
					 idx + (q >> 10) % 4096);
			break;
		case 4:
			e = mas_find_rev(&mas, (q & 0x80) ? 0 :
					 gen_idx(q >> 12) % 256);
			break;
		case 5:
			e = mas_next_range(&mas, (q & 0x80) ? ULONG_MAX :
					 idx + (q >> 10) % 4096);
			break;
		case 6:
			e = mas_prev_range(&mas, (q & 0x80) ? 0 :
					 gen_idx(q >> 12) % 256);
			break;
		default:
			e = mas_find_range(&mas, (q & 0x80) ? ULONG_MAX :
					 idx + (q >> 10) % 4096);
			break;
		}
		fold(efold(e));
		fold_mas(&mas);
		if (r & 0x40000)
			mas_pause(&mas);
		else if (r & 0x80000)
			mas_reset(&mas);
		if (mas.status == ma_overflow || mas.status == ma_underflow)
			mas_set(&mas, gen_idx(q >> 20));
	}
	mas_destroy(&mas);
}

/* mas_for_each / mas_for_each_rev bounded iteration */
static void op_for_each(uint64_t r)
{
	unsigned long idx, lim;
	MA_STATE(mas, &mt, 0, 0);
	void *e;
	int k = 0;

	if (r & 0x20) {
		idx = gen_idx(r);
		lim = (r & 0x40) ? ULONG_MAX : idx + (r >> 16) % 8192;
		mas_set(&mas, idx);
		mas_for_each(&mas, e, lim) {
			fold(efold(e));
			fold(mas.index);
			fold(mas.last);
			if (++k > 48)
				break;
		}
		fold(NILX);
		fold(k);
	} else {
		idx = gen_idx(r);
		lim = (r & 0x40) ? 0 : idx % 512;
		mas_set(&mas, idx);
		mas_for_each_rev(&mas, e, lim) {
			fold(efold(e));
			fold(mas.index);
			fold(mas.last);
			if (++k > 48)
				break;
		}
		fold(NILX);
		fold(k);
	}
	mas_destroy(&mas);
}

static void op_empty_area(uint64_t r)
{
	unsigned long min, max;
	unsigned long size;
	MA_STATE(mas, &mt, 0, 0);
	int ret;

	gen_range(r, &min, &max);
	size = 1 + ((r >> 21) % 48);
	if (r & 0x400)
		size = 1 + ((r >> 25) % 1024);
	if (r & 0x2000) {
		ret = mas_empty_area_rev(&mas, min, max, size);
		fold((uint64_t)(int64_t)ret);
		fold_mas(&mas);
	} else {
		ret = mas_empty_area(&mas, min, max, size);
		fold((uint64_t)(int64_t)ret);
		fold_mas(&mas);
	}
	mas_destroy(&mas);
}

/* preallocate + store_prealloc round trip (occasionally destroy instead) */
static void op_prealloc(uint64_t r)
{
	unsigned long s, l;
	void *e = gen_entry_nonnull(r >> 20);
	MA_STATE(mas, &mt, 0, 0);
	int ret;

	gen_range(r, &s, &l);
	mas_set_range(&mas, s, l);
	ret = mas_preallocate(&mas, e, gen_gfp(r >> 40));
	fold((uint64_t)(int64_t)ret);
	fold_mas(&mas);
	if (!ret && !(r & 0x800)) {
		mas_store_prealloc(&mas, e);
		fold_mas(&mas);
		hot_note(s, l);
	} else {
		mas_destroy(&mas);
		fold(NILX);
	}
}

/* mtree_dup into mt2 (or __mt_dup), then operate/destroy on the copy */
static void op_dup(uint64_t r, unsigned int flags)
{
	int ret;
	unsigned long idx;

	mt_init_flags(&mt2, flags);
	if (r & 0x100)
		ret = __mt_dup(&mt, &mt2, gen_gfp(r >> 20));
	else
		ret = mtree_dup(&mt, &mt2, gen_gfp(r >> 20));
	fold((uint64_t)(int64_t)ret);
	dump_both();
	if (r & 0x200) {
		idx = gen_idx(r >> 10);
		fold(efold(mtree_load(&mt2, idx)));
		fold(efold(mtree_erase(&mt2, idx)));
	}
	if (r & 0x400)
		fold(efold(mtree_erase(&mt2, gen_idx(r >> 14))));
	dump_both();
	mtree_destroy(&mt2);
	fold(efold(mt2.ma_root));
	mt_init_flags(&mt2, flags);
}

static void op_destroy(unsigned int flags)
{
	mtree_destroy(&mt);
	fold(efold(mt.ma_root));
	fold(mt.ma_flags);
	memset(hot, 0, sizeof hot);
	mt_init_flags(&mt, flags);
	fold(flags);
}

static void op_mt_find(uint64_t r)
{
	unsigned long idx;
	void *e;

	if ((r >> 4 & 1) && hot_pick(r >> 9, &idx, &idx)) {
		;
	} else {
		idx = gen_idx(r);
	}
	switch ((r >> 3) % 4) {
	case 0:
		e = mt_find(&mt, &idx,
			    (r & 0x1000) ? ULONG_MAX : idx + (r >> 20) % 4096);
		fold(efold(e));
		fold(idx);
		break;
	case 1:
		idx++;
		if (!idx)
			fold(NILX);
		e = mt_find_after(&mt, &idx,
			    (r & 0x1000) ? ULONG_MAX : idx + (r >> 20) % 4096);
		fold(efold(e));
		fold(idx);
		break;
	case 2:
		e = mt_next(&mt, idx,
			    (r & 0x1000) ? ULONG_MAX : idx + (r >> 20) % 4096);
		fold(efold(e));
		break;
	default:
		e = mt_prev(&mt, idx, (r & 0x1000) ? 0 : idx % 256);
		fold(efold(e));
		break;
	}
}

/* ================= wrap_mt_seq: general op mix ================= */

uint64_t wrap_mt_seq(uint64_t seed, int64_t nops_i, int64_t seed2)
{
	static const int mix[8] = {22, 10, 12, 8, 6, 14, 16, 12};
	int nops = (int)(iabs(nops_i) % 180);
	unsigned int flags = flag_tab[iabs(seed2) % 6];
	int dump_every = 4 + (int)(iabs(seed2) % 7);
	int i;

	reset_all(flags);
	memset(hot, 0, sizeof hot);
	rng_state = seed | 1;
	fail_mask = iabs(seed2) ? iabs(seed2) * 0x9e3779b97f4a7c15ULL : 0;
	if (iabs(seed2) % 5 == 1)
		fail_mask = ~0ULL;	/* every early alloc fails */

	for (i = 0; i < nops; i++) {
		uint64_t r = xs64();
		int t = (int)(r % 100), c = 0;
		if (t < (c += mix[0])) {
			switch ((r >> 4) % 4) {
			case 0: case 1:
				op_store(r);
				break;
			case 2:
				op_store_range(r);
				break;
			default:
				op_insert(r);
				break;
			}
		} else if (t < (c += mix[1])) {
			op_load(r);
		} else if (t < (c += mix[2])) {
			op_erase(r);
		} else if (t < (c += mix[3])) {
			op_prealloc(r);
		} else if (t < (c += mix[4])) {
			op_mt_find(r);
		} else if (t < (c += mix[5])) {
			op_walk(r);
		} else if (t < (c += mix[6])) {
			switch ((r >> 4) % 4) {
			case 0:
				op_for_each(r);
				break;
			case 1:
				op_empty_area(r);
				break;
			case 2: {
				unsigned long lo, hi, startp = 0xbad;
				gen_range(r, &lo, &hi);
				if (r & 0x80) {
					unsigned long next =
						lo + (r >> 16) % 64;
					fold((uint64_t)(int64_t)
					     mtree_alloc_cyclic(&mt, &startp,
						gen_entry_nonnull(r >> 30),
						lo, hi, &next,
						gen_gfp(r >> 40)));
					fold(next);
				} else if (r & 0x100) {
					fold((uint64_t)(int64_t)
					     mtree_alloc_rrange(&mt, &startp,
						gen_entry_nonnull(r >> 30),
						1 + (r >> 12) % 32, lo, hi,
						gen_gfp(r >> 40)));
				} else {
					fold((uint64_t)(int64_t)
					     mtree_alloc_range(&mt, &startp,
						gen_entry_nonnull(r >> 30),
						1 + (r >> 12) % 32, lo, hi,
						gen_gfp(r >> 40)));
				}
				fold(startp);
				break;
			}
			default:
				op_walk(r + 0x9e3779b9ULL);
				break;
			}
		} else {
			switch ((r >> 4) % 4) {
			case 0:
				op_dup(r, flags);
				break;
			case 1:
				op_destroy(flags);
				break;
			case 2:
				op_for_each(r ^ 0x5a5a5a5aULL);
				break;
			default:
				op_empty_area(r ^ 0x1234567ULL);
				break;
			}
		}
		if (i % dump_every == dump_every - 1)
			dump_both();
	}
	dump_both();
	fold(kshim_warn_hits);
	return g_h;
}

/* ================= wrap_mt_area: alloc-tree / gap stress ============ */

uint64_t wrap_mt_area(uint64_t seed, int64_t nops_i, int64_t seed2)
{
	int nops = (int)(iabs(nops_i) % 160);
	unsigned int flags = MT_FLAGS_ALLOC_RANGE |
		((iabs(seed2) & 3) == 1 ? MT_FLAGS_USE_RCU : 0) |
		((iabs(seed2) & 7) == 3 ? MT_FLAGS_LOCK_EXTERN : 0);
	int dump_every = 4 + (int)(iabs(seed2) % 6);
	int i;

	reset_all(flags);
	memset(hot, 0, sizeof hot);
	rng_state = seed | 1;
	fail_mask = iabs(seed2) ? iabs(seed2) * 0x9e3779b97f4a7c15ULL : 0;
	if (iabs(seed2) % 5 == 1)
		fail_mask = ~0ULL;

	for (i = 0; i < nops; i++) {
		uint64_t r = xs64();
		unsigned long lo, hi, startp = 0xbad;
		MA_STATE(mas, &mt, 0, 0);

		switch ((int)(r % 20)) {
		case 0: case 1: case 2: case 3:
			gen_range(r, &lo, &hi);
			fold((uint64_t)(int64_t)
			     mtree_alloc_range(&mt, &startp,
					gen_entry_nonnull(r >> 30),
					1 + (r >> 12) % 32, lo, hi,
					gen_gfp(r >> 40)));
			fold(startp);
			hot_note(startp, startp + (r >> 12) % 32);
			break;
		case 4: case 5:
			gen_range(r, &lo, &hi);
			fold((uint64_t)(int64_t)
			     mtree_alloc_rrange(&mt, &startp,
					gen_entry_nonnull(r >> 30),
					1 + (r >> 12) % 32, lo, hi,
					gen_gfp(r >> 40)));
			fold(startp);
			hot_note(startp, startp + (r >> 12) % 32);
			break;
		case 6: case 7: {
			unsigned long next = (r >> 16) % 128;
			gen_range(r, &lo, &hi);
			fold((uint64_t)(int64_t)
			     mtree_alloc_cyclic(&mt, &startp,
					gen_entry_nonnull(r >> 30),
					lo, hi, &next, gen_gfp(r >> 40)));
			fold(startp);
			fold(next);
			hot_note(startp, startp);
			break;
		}
		case 8: case 9:
			gen_range(r, &lo, &hi);
			fold((uint64_t)(int64_t)
			     mas_empty_area(&mas, lo, hi,
					    1 + (r >> 12) % 64));
			fold(mas.index);
			fold(mas.last);
			mas_destroy(&mas);
			break;
		case 10: case 11:
			gen_range(r, &lo, &hi);
			fold((uint64_t)(int64_t)
			     mas_empty_area_rev(&mas, lo, hi,
						1 + (r >> 12) % 64));
			fold(mas.index);
			fold(mas.last);
			mas_destroy(&mas);
			break;
		case 12: case 13:
			op_store(r);
			break;
		case 14:
			op_erase(r);
			break;
		case 15:
			op_load(r);
			break;
		case 16:
			op_for_each(r);
			break;
		case 17:
			op_destroy(flags);
			break;
		case 18:
			/* fill a window densely then carve holes */
			gen_range(r, &lo, &hi);
			hi = lo + (hi - lo) % 40;
			fold((uint64_t)(int64_t)
			     mtree_store_range(&mt, lo, hi,
					gen_entry_nonnull(r >> 30),
					gen_gfp(r >> 40)));
			if (r & 0x4000)
				fold(efold(mtree_erase(&mt,
						lo + (r >> 20) % 40)));
			hot_note(lo, hi);
			break;
		default:
			op_walk(r);
			break;
		}
		if (i % dump_every == dump_every - 1)
			dump_both();
	}
	dump_both();
	fold(kshim_warn_hits);
	return g_h;
}

/* ================= wrap_mt_iter: traversal ========================= */

uint64_t wrap_mt_iter(uint64_t seed, int64_t n_i, int64_t seed2)
{
	int n = (int)(iabs(n_i) % 120);
	int walk = (int)(iabs(seed2) % 4);
	unsigned int flags = flag_tab[iabs(seed2) % 6];
	int i;

	reset_all(flags);
	memset(hot, 0, sizeof hot);
	rng_state = seed | 1;
	fail_mask = 0;

	/* build phase */
	for (i = 0; i < n; i++) {
		uint64_t r = xs64();
		unsigned long s, l;
		if (r % 4 == 0) {
			gen_range(r, &s, &l);
			fold((uint64_t)(int64_t)
			     mtree_store_range(&mt, s, l,
					gen_entry_nonnull(r >> 30),
					GFP_KERNEL));
			hot_note(s, l);
		} else {
			fold((uint64_t)(int64_t)
			     mtree_store(&mt, gen_idx(r),
					gen_entry_nonnull(r >> 9), GFP_KERNEL));
		}
		if (r % 11 == 0)
			fold(efold(mtree_erase(&mt, gen_idx(r >> 25))));
	}
	dump_both();

	/* walk phase */
	if (walk == 0) {
		unsigned long index = 0, max;
		void *entry;
		int steps = 0;

		max = (seed2 & 0x100) ? ULONG_MAX : gen_idx(xs64()) % (1UL << 24);
		for (entry = mt_find(&mt, &index, max); entry;
		     entry = mt_find_after(&mt, &index, max)) {
			fold(index);
			fold(efold(entry));
			if (++steps > 80 || index == ULONG_MAX)
				break;
			index++;
		}
		fold(NILX);
		fold(steps > 80);
	} else if (walk == 1) {
		/* mas_for_each from a random start, pausing periodically */
		MA_STATE(mas, &mt, 0, 0);
		unsigned long max;
		void *entry;
		int steps = 0;

		mas_set(&mas, gen_idx(xs64()) % 4096);
		max = (seed2 & 0x200) ? ULONG_MAX : (1UL << 20);
		mas_for_each(&mas, entry, max) {
			fold(efold(entry));
			fold(mas.index);
			fold(mas.last);
			if (++steps % 3 == 0)
				mas_pause(&mas);
			if (steps > 60)
				break;
		}
		fold(NILX);
		fold(steps > 60);
		mas_destroy(&mas);
	} else if (walk == 2) {
		/* sequential next/prev through holes, then reverse */
		MA_STATE(mas, &mt, 0, 0);
		void *e;
		int k;
		unsigned long start = gen_idx(xs64()) % 512;

		mas_set(&mas, start);
		for (k = 0; k < 90; k++) {
			e = mas_next(&mas, ULONG_MAX);
			fold(efold(e));
			fold(mas.index);
			if (mas.status == ma_overflow)
				break;
		}
		fold(NILX);
		mas_set(&mas, mas.index);
		for (k = 0; k < 90; k++) {
			e = mas_prev(&mas, 0);
			fold(efold(e));
			fold(mas.index);
			if (mas.status == ma_underflow)
				break;
		}
		fold(NILX);
		mas_destroy(&mas);
	} else {
		/* destructive: erase every other entry mid-walk */
		MA_STATE(mas, &mt, 0, 0);
		void *entry;
		int k = 0;

		mas_set(&mas, 0);
		for (;;) {
			entry = mas_find(&mas, ULONG_MAX);
			if (!entry)
				break;
			fold(mas.index);
			fold(mas.last);
			if (k++ % 2 == 0) {
				MA_STATE(em, &mt, mas.index, mas.index);
				fold(efold(mas_erase(&em)));
			}
			if (k > 120 || mas.index == ULONG_MAX)
				break;
			mas_set(&mas, mas.index + 1);
		}
		fold(NILX);
		mas_destroy(&mas);
	}
	dump_both();
	fold(kshim_warn_hits);
	return g_h;
}
