/* Scalar wrappers around lib/assoc_array.c (a byte-identical upstream
 * copy) so the standard scalar bridge can drive it.
 *
 * Address-space model: the assoc_array unit stores real C pointers —
 * tagged assoc_array_ptr values (low bit = meta, second bit = shortcut)
 * and raw object/node/shortcut pointers — so pointer identity must be
 * foldable.  Everything the unit can allocate lives in a static pool:
 *
 *   node_pool[NP]    struct assoc_array_node      (kzalloc_obj/kfree)
 *   sc_pool[SP]      struct scell = assoc_array_shortcut + ik[SCFLEX]
 *                    (kzalloc_flex/kmalloc_flex with keylen <= SCFLEX)
 *   edit_pool[NE]    struct assoc_array_edit      (kzalloc_obj/kfree)
 *   opool[NO]        struct tobj leaf objects     (harness-owned; the
 *                    unit only ever frees them via ops->free_object)
 *
 * kmalloc/kzalloc route by request size; kfree routes by pool address
 * range.  One shared alloc ordinal stream (alloc_ctr) plus a per-case
 * fail_mask nominates which alloc calls return NULL, driving the real
 * -ENOMEM paths in insert/delete/gc deterministically.
 *
 * canon() maps every tagged pointer to ONE canonical u64 — the exact
 * encoding the Rust port uses as its pointer values:
 *   NULL                       -> 0
 *   &opool[i] (leaf, tag 0)    -> LCANON + i*16
 *   node_pool[i]  (tag 01)     -> NCANON + i*0x1000 + 1
 *   sc_pool[j].s  (tag 11)     -> SCANON + j*0x800  + 3
 *   &edit_pool[k]              -> EDCANON + k*0x800
 *
 * ppc() canonicalises struct assoc_array_ptr ** slot addresses:
 *   &array.root                -> PC_ROOT
 *   &node_pool[i].slots[s]     -> PC_NSLOT + i*64 + s
 *   &node_pool[i].back_pointer -> PC_NBACK + i
 *   &sc_pool[j].s.next_node    -> PC_SNEXT + j
 *   &sc_pool[j].s.back_pointer -> PC_SBACK + j
 * and ppc8() canonicalises u8 * parent_slot addresses:
 *   &node_pool[i].parent_slot  -> PC_NPSLT + i
 *
 * Index keys are two-word (128-bit) keys; leaf objects are tobj structs
 * carrying the same two words plus a refcount.  The ops table mirrors
 * keyring_assoc_array_ops semantics: chunk = w[level/64] (0 past the
 * key), diff_objects = word*64 + __ffs(xor) of the first differing word
 * or -1 when equal, free_object drops a ref and folds.
 *
 * Every wrapper returns one order-sensitive FNV-1a-style checksum
 * folded over each op's inputs/results, a full fold of every edit
 * script (all pointers canonicalised) before it is applied/cancelled,
 * and a periodic FULL STATE DUMP of the root, every live node, every
 * live shortcut and every live object.  Raw addresses never fold.
 */
#include <stdint.h>
#include <stddef.h>
#include <string.h>
#include <stdio.h>
#include <stdlib.h>
#include <linux/types.h>
#include <linux/compiler.h>
#include <linux/kernel.h>
#include <linux/bug.h>
#include <linux/err.h>
#include <linux/errno.h>
#include <linux/slab.h>
#include <linux/rcupdate.h>
#include <linux/assoc_array.h>
#include <linux/assoc_array_priv.h>

#define NP	320	/* assoc_array_node pool slots */
#define SP	200	/* shortcut pool slots */
#define NE	8	/* edit script pool slots (never live simultaneously) */
#define NO	96	/* leaf objects */
#define KW	2	/* words per index key (128-bit key space) */
#define SCFLEX	4	/* shortcut pool slot index_key capacity */
#define NILX	0xffffULL

/* canonical encodings shared with port.rs */
#define LCANON	0x10000ULL	/* leaf canonical base, stride 16 */
#define NCANON	0x40000ULL	/* node canonical base, stride 0x1000, |1 */
#define SCANON	0x200000ULL	/* shortcut canonical base, stride 0x800, |3 */
#define EDCANON	0x600000ULL	/* edit canonical base, stride 0x800 */
#define PC_ROOT	0x810000ULL	/* &array.root */
#define PC_NSLOT 0xB10000ULL	/* node slot address canonicals, stride 64 */
#define PC_NBACK 0xB20000ULL	/* node back_pointer address canonicals */
#define PC_NPSLT 0xB30000ULL	/* node parent_slot address canonicals */
#define PC_SNEXT 0xB40000ULL	/* shortcut next_node address canonicals */
#define PC_SBACK 0xB50000ULL	/* shortcut back_pointer address canonicals */

struct tkey {
	unsigned long w[KW];
};

/* A leaf object: what upstream callers hang off the tree (struct key in
 * keyrings).  refs models the kernel refcount: inserting hands the
 * tree a ref, free_object drops one; the gc iterator must take a ref
 * for every object it keeps (the upstream contract). */
struct tobj {
	struct tkey	key;
	unsigned long	serial;
	long		refs;
};

struct scell {
	struct assoc_array_shortcut s;
	unsigned long	ik[SCFLEX];
};

static struct assoc_array_node node_pool[NP];
static uint8_t node_used[NP];
static struct scell sc_pool[SP];
static uint8_t sc_used[SP];
static struct assoc_array_edit edit_pool[NE];
static uint8_t edit_used[NE];
static struct tobj opool[NO];
static struct assoc_array array;

/* ---- deterministic allocator (slab boundary, see linux/slab.h) ---- */

static unsigned long alloc_ctr;
static uint64_t fail_mask;

/* one shared ordinal stream for every allocation the unit performs */
static int alloc_fail_check(void)
{
	alloc_ctr++;
	if (alloc_ctr <= 64 && (fail_mask >> (alloc_ctr - 1)) & 1)
		return 1;
	return 0;
}

static void *pool_alloc(size_t size)
{
	int i;

	if (alloc_fail_check())
		return NULL;
	if (size == sizeof(struct assoc_array_node)) {
		for (i = 0; i < NP; i++)
			if (!node_used[i]) {
				node_used[i] = 1;
				return &node_pool[i];
			}
		return NULL;
	}
	if (size == sizeof(struct assoc_array_edit)) {
		for (i = 0; i < NE; i++)
			if (!edit_used[i]) {
				edit_used[i] = 1;
				return &edit_pool[i];
			}
		return NULL;
	}
	if (size >= sizeof(struct assoc_array_shortcut) &&
	    size <= sizeof(struct scell)) {
		for (i = 0; i < SP; i++)
			if (!sc_used[i]) {
				sc_used[i] = 1;
				return &sc_pool[i];
			}
		return NULL;
	}
	BUG();
	return NULL;
}

void *kmalloc(size_t size, gfp_t gfp)
{
	(void)gfp;
	return pool_alloc(size);
}

void *kzalloc(size_t size, gfp_t gfp)
{
	void *p = pool_alloc(size);
	if (p)
		memset(p, 0, size);
	return p;
}

void kfree(const void *ptr)
{
	const char *p = ptr;
	size_t off;
	int i;

	if (!ptr)
		return;
	if (p >= (const char *)node_pool &&
	    p < (const char *)(node_pool + NP)) {
		off = p - (const char *)node_pool;
		BUG_ON(off % sizeof(struct assoc_array_node) != 0);
		i = (int)(off / sizeof(struct assoc_array_node));
		BUG_ON(!node_used[i]);
		node_used[i] = 0;
		return;
	}
	if (p >= (const char *)sc_pool &&
	    p < (const char *)(sc_pool + SP)) {
		off = p - (const char *)sc_pool;
		BUG_ON(off % sizeof(struct scell) != 0);
		i = (int)(off / sizeof(struct scell));
		BUG_ON(!sc_used[i]);
		sc_used[i] = 0;
		return;
	}
	if (p >= (const char *)edit_pool &&
	    p < (const char *)(edit_pool + NE)) {
		off = p - (const char *)edit_pool;
		BUG_ON(off % sizeof(struct assoc_array_edit) != 0);
		i = (int)(off / sizeof(struct assoc_array_edit));
		BUG_ON(!edit_used[i]);
		edit_used[i] = 0;
		return;
	}
	BUG();
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

/* ---- canonicalisation ---- */

static int node_idx(const struct assoc_array_node *n)
{
	return (int)(n - node_pool);
}
static int sc_idx(const struct assoc_array_shortcut *s)
{
	return (int)((const struct scell *)s - sc_pool);
}
static int obj_idx(const void *o)
{
	return (int)((const struct tobj *)o - opool);
}
static int edit_idx(const struct assoc_array_edit *e)
{
	return (int)(e - edit_pool);
}

/* tagged assoc_array_ptr -> canonical u64 (Rust's pointer encoding) */
static uint64_t canon(const struct assoc_array_ptr *p)
{
	uintptr_t a;

	if (!p)
		return 0;
	a = (uintptr_t)p & ~3ULL;
	if (!assoc_array_ptr_is_meta(p)) {
		if ((const struct tobj *)a >= opool &&
		    (const struct tobj *)a < opool + NO)
			return LCANON + (uint64_t)obj_idx((void *)a) * 16;
		return 0xdead0ULL;
	}
	if ((void *)a >= (void *)node_pool &&
	    (void *)a < (void *)(node_pool + NP))
		return NCANON +
			(uint64_t)node_idx((const void *)a) * 0x1000 + 1;
	if ((void *)a >= (void *)sc_pool &&
	    (void *)a < (void *)(sc_pool + SP))
		return SCANON +
			(uint64_t)((const struct scell *)a - sc_pool) * 0x800 + 3;
	return 0xdead1ULL;
}

/* raw struct assoc_array_node * -> canonical (untagged) */
static uint64_t ncanon(const struct assoc_array_node *n)
{
	return n ? NCANON + (uint64_t)node_idx(n) * 0x1000 : 0;
}

/* struct assoc_array_ptr ** -> canonical u64 */
static uint64_t ppc(struct assoc_array_ptr * const *p)
{
	uintptr_t a = (uintptr_t)p, off;
	size_t ni, within;

	if (!p)
		return 0;
	if (p == &array.root)
		return PC_ROOT;
	if (a >= (uintptr_t)node_pool &&
	    a < (uintptr_t)node_pool + sizeof(node_pool)) {
		off = a - (uintptr_t)node_pool;
		ni = off / sizeof(struct assoc_array_node);
		within = off % sizeof(struct assoc_array_node);
		if (within == offsetof(struct assoc_array_node, back_pointer))
			return PC_NBACK + ni;
		if (within >= offsetof(struct assoc_array_node, slots) &&
		    within < offsetof(struct assoc_array_node, slots) +
			    sizeof(node_pool[0].slots) &&
		    (within - offsetof(struct assoc_array_node, slots)) %
			    sizeof(void *) == 0)
			return PC_NSLOT + ni * 64 +
				(within - offsetof(struct assoc_array_node,
						  slots)) / sizeof(void *);
		return 0xdead2ULL;
	}
	if (a >= (uintptr_t)sc_pool &&
	    a < (uintptr_t)sc_pool + sizeof(sc_pool)) {
		off = a - (uintptr_t)sc_pool;
		ni = off / sizeof(struct scell);
		within = off % sizeof(struct scell);
		if (within == offsetof(struct assoc_array_shortcut, next_node))
			return PC_SNEXT + ni;
		if (within == offsetof(struct assoc_array_shortcut,
				       back_pointer))
			return PC_SBACK + ni;
		return 0xdead3ULL;
	}
	return 0xdead4ULL;
}

/* u8 * parent_slot address -> canonical u64 */
static uint64_t ppc8(const u8 *p)
{
	uintptr_t a = (uintptr_t)p, off;
	size_t ni, within;

	if (!p)
		return 0;
	if (a >= (uintptr_t)node_pool &&
	    a < (uintptr_t)node_pool + sizeof(node_pool)) {
		off = a - (uintptr_t)node_pool;
		ni = off / sizeof(struct assoc_array_node);
		within = off % sizeof(struct assoc_array_node);
		if (within == offsetof(struct assoc_array_node, parent_slot))
			return PC_NPSLT + ni;
	}
	return 0xdead5ULL;
}

/* ---- the ops table (keyring_assoc_array_ops semantics) ---- */

static unsigned long t_get_key_chunk(const void *index_key, int level)
{
	const struct tkey *k = index_key;
	unsigned int w = (unsigned int)level / ASSOC_ARRAY_KEY_CHUNK_SIZE;
	return w < KW ? k->w[w] : 0;
}

static unsigned long t_get_object_key_chunk(const void *object, int level)
{
	const struct tobj *o = object;
	unsigned int w = (unsigned int)level / ASSOC_ARRAY_KEY_CHUNK_SIZE;
	return w < KW ? o->key.w[w] : 0;
}

static bool t_compare_object(const void *object, const void *index_key)
{
	const struct tobj *o = object;
	const struct tkey *k = index_key;
	return memcmp(o->key.w, k->w, sizeof k->w) == 0;
}

static int t_diff_objects(const void *object, const void *index_key)
{
	const struct tobj *o = object;
	const struct tkey *k = index_key;
	unsigned long d;
	int i;

	for (i = 0; i < KW; i++) {
		d = o->key.w[i] ^ k->w[i];
		if (d)
			return i * BITS_PER_LONG + __ffs64(d);
	}
	return -1;
}

static void t_free_object(void *object)
{
	struct tobj *o = object;
	int i;

	BUG_ON(o < opool || o >= opool + NO);
	i = obj_idx(o);
	fold(0xfee1);
	fold((uint64_t)i);
	o->refs--;
	fold((uint64_t)o->refs);
}

static const struct assoc_array_ops tops = {
	.get_key_chunk		= t_get_key_chunk,
	.get_object_key_chunk	= t_get_object_key_chunk,
	.compare_object		= t_compare_object,
	.diff_objects		= t_diff_objects,
	.free_object		= t_free_object,
};

/* ---- generators ---- */

/* Index keys.  The tree segments a key 4 bits at a time starting at bit
 * 0, so keys that differ only in high bits force all-one-segment splits
 * and shortcuts; dense low keys fill nodes and force ordinary splits;
 * the tiny fixed set gives frequent exact-key duplicates (the replace
 * path). */
static void gen_key(struct tkey *k, uint64_t r)
{
	switch (r % 8) {
	case 0: case 1:			/* dense low keys */
		k->w[0] = (r >> 8) % 48;
		k->w[1] = 0;
		break;
	case 2:				/* mid keys */
		k->w[0] = (r >> 8) % 65536;
		k->w[1] = 0;
		break;
	case 3:				/* common low 32 bits */
		k->w[0] = 0x12345UL | (((r >> 8) % 256) << 32);
		k->w[1] = (r >> 16) % 6;
		break;
	case 4:				/* common low 40 bits */
		k->w[0] = 0xABCDEF0123UL | (((r >> 8) % 1024) << 40);
		k->w[1] = (r >> 24) % 8;
		break;
	case 5:				/* differ only in w1 low bits */
		k->w[0] = 0x777777777777777UL;
		k->w[1] = (r >> 8) % 24;
		break;
	case 6:				/* tiny fixed set: duplicates */
		k->w[0] = 1 + (r >> 8) % 4;
		k->w[1] = 0;
		break;
	default:			/* full-width scatter */
		k->w[0] = (r >> 8) & 0xffffffffUL;
		k->w[1] = (r >> 40) & 0xffffffUL;
		break;
	}
}

/* A key that is (probably) live in the tree: pick the want-th object
 * with refs > 0.  Returns 1 when a live key was produced. */
static int live_key(struct tkey *k, uint64_t r)
{
	int want = 1 + (int)((r >> 13) % 8);
	int i;

	for (i = 0; i < NO; i++)
		if (opool[i].refs > 0 && --want == 0) {
			*k = opool[i].key;
			return 1;
		}
	return 0;
}

static int pick_free_obj(uint64_t r)
{
	int i, start = (int)((r >> 8) % NO);

	for (i = 0; i < NO; i++) {
		int k = (start + i) % NO;
		if (opool[k].refs == 0)
			return k;
	}
	return -1;
}

/* ---- edit-script folding ---- */

static void fold_edit(const struct assoc_array_edit *e)
{
	int i;

	fold(EDCANON + (uint64_t)edit_idx(e) * 0x800);
	fold(e->ops != NULL);
	fold(e->ops_for_excised_subtree != NULL);
	fold(canon(e->leaf));
	fold(ppc(e->leaf_p));
	fold(canon(e->dead_leaf));
	for (i = 0; i < 3; i++)
		fold(canon(e->new_meta[i]));
	for (i = 0; i < 2; i++)
		fold(canon(e->excised_meta[i]));
	fold(canon(e->excised_subtree));
	for (i = 0; i < ASSOC_ARRAY_FAN_OUT; i++)
		fold(ppc(e->set_backpointers[i]));
	fold(canon(e->set_backpointers_to));
	fold(ncanon(e->adjust_count_on));
	fold((uint64_t)e->adjust_count_by);
	for (i = 0; i < 2; i++) {
		fold(ppc(e->set[i].ptr));
		fold(canon(e->set[i].to));
	}
	fold(ppc8(e->set_parent_slot[0].p));
	fold(e->set_parent_slot[0].to);
	for (i = 0; i < ASSOC_ARRAY_FAN_OUT + 1; i++)
		fold(e->segment_cache[i]);
}

/* ---- full-state dump ---- */

static void dump_state(void)
{
	int i, s, nused = 0, sused = 0, live = 0;

	fold(canon(array.root));
	fold(array.nr_leaves_on_tree);

	for (i = 0; i < NP; i++)
		nused += node_used[i];
	fold(nused);
	for (i = 0; i < NP; i++) {
		struct assoc_array_node *n = &node_pool[i];
		if (!node_used[i])
			continue;
		fold(i);
		fold(n->parent_slot);
		fold(canon(n->back_pointer));
		fold(n->nr_leaves_on_branch);
		for (s = 0; s < ASSOC_ARRAY_FAN_OUT; s++)
			fold(canon(n->slots[s]));
	}
	fold(NILX);

	for (i = 0; i < SP; i++)
		sused += sc_used[i];
	fold(sused);
	for (i = 0; i < SP; i++) {
		struct assoc_array_shortcut *sc = &sc_pool[i].s;
		int kl;
		if (!sc_used[i])
			continue;
		fold(i);
		fold((uint64_t)sc->parent_slot);
		fold((uint64_t)sc->skip_to_level);
		fold(canon(sc->back_pointer));
		fold(canon(sc->next_node));
		kl = (int)(round_up((unsigned long)sc->skip_to_level,
				    ASSOC_ARRAY_KEY_CHUNK_SIZE) >>
			   ASSOC_ARRAY_KEY_CHUNK_SHIFT);
		if (kl > SCFLEX)
			kl = SCFLEX;
		for (s = 0; s < kl; s++)
			fold(sc->index_key[s]);
	}
	fold(NILX);

	for (i = 0; i < NO; i++)
		live += opool[i].refs > 0;
	fold(live);
	for (i = 0; i < NO; i++) {
		if (opool[i].refs <= 0)
			continue;
		fold(i);
		fold((uint64_t)opool[i].refs);
		fold(opool[i].key.w[0]);
		fold(opool[i].key.w[1]);
	}
	fold(NILX);
}

static void reset_all(void)
{
	int i;

	memset(node_pool, 0, sizeof node_pool);
	memset(node_used, 0, sizeof node_used);
	memset(sc_pool, 0, sizeof sc_pool);
	memset(sc_used, 0, sizeof sc_used);
	memset(edit_pool, 0, sizeof edit_pool);
	memset(edit_used, 0, sizeof edit_used);
	for (i = 0; i < NO; i++) {
		opool[i].key.w[0] = 0;
		opool[i].key.w[1] = 0;
		opool[i].serial = i;
		opool[i].refs = 0;
	}
	assoc_array_init(&array);
	alloc_ctr = 0;
	g_h = 0x9e3779b97f4a7c15ULL;
}

/* ================= ops ================= */

/* Key families whose members share a long common low-bit prefix.
 * Enough inserts from one family pile its keys into one node, force an
 * all-one-segment split and so create shortcuts — the pathological
 * shape assoc_array_insert_shortcut/_mid_shortcut and the gc shortcut
 * handling exist for. */
static void cluster_key(struct tkey *k, unsigned int fam, uint64_t v)
{
	switch (fam & 3) {
	case 0:		/* share the low 20 bits */
		k->w[0] = 0x20UL | ((v % (1UL << 24)) << 20);
		k->w[1] = v % 64;
		break;
	case 1:		/* share the low 32 bits */
		k->w[0] = 0x777UL | ((v % (1UL << 16)) << 32);
		k->w[1] = v % 32;
		break;
	case 2:		/* share the whole of w0 (low 64 bits) */
		k->w[0] = 0x777777777777777UL;
		k->w[1] = v % 96;
		break;
	default:	/* share the low 40 bits */
		k->w[0] = 0xABCDEF0123UL | ((v % 256) << 40);
		k->w[1] = v % 16;
		break;
	}
}

static void op_insert_key(const struct tkey *k, uint64_t r)
{
	struct assoc_array_edit *edit;
	int oi, held;

	oi = pick_free_obj(r >> 28);
	if (oi < 0) {
		fold(NILX);
		return;
	}
	held = oi;
	opool[oi].key = *k;	/* the object carries its index key */
	opool[oi].refs++;

	edit = assoc_array_insert(&array, &tops, k, &opool[oi]);
	if (IS_ERR(edit)) {
		/* caller keeps its object ref on failure */
		fold((uint64_t)PTR_ERR(edit));
		opool[held].refs--;
		return;
	}
	fold(0xe1);
	fold_edit(edit);

	if (r & 0x8000) {
		/* sometimes substitute a different object before apply */
		int oi2 = pick_free_obj(r >> 40);
		fold(oi2 < 0 ? NILX : (uint64_t)oi2);
		if (oi2 >= 0) {
			opool[oi2].key = *k;
			opool[oi2].refs++;
			assoc_array_insert_set_object(edit, &opool[oi2]);
			opool[oi].refs--;	/* caller drops the first */
			held = oi2;
			fold(canon(edit->leaf));
		}
	}

	if (r & 0x10000) {
		fold(0xca9ce1);
		assoc_array_cancel_edit(edit);
		opool[held].refs--;	/* cancel does not free the leaf */
	} else {
		fold(0xa991e);
		assoc_array_apply_edit(edit);
	}
}

static void op_insert(uint64_t r)
{
	struct tkey k;

	gen_key(&k, r >> 4);
	op_insert_key(&k, r);
}

static void op_insert_burst(uint64_t r)
{
	int n = 1 + (int)((r >> 10) % 4);
	int i;

	for (i = 0; i < n; i++) {
		uint64_t r2 = xs64();
		struct tkey k;
		gen_key(&k, r2 >> 4);
		op_insert_key(&k, r2 & ~0x18000ULL);	/* always apply */
	}
}

/* A burst of inserts from one shared-prefix family: the reliable way to
 * build up a full same-segment node and force shortcut creation. */
static void op_cluster(uint64_t r)
{
	unsigned int fam = (unsigned int)((r >> 4) % 4);
	uint64_t base_v = r >> 12;
	int n = 4 + (int)((r >> 24) % 9);	/* 4..12 inserts */
	int i;

	for (i = 0; i < n; i++) {
		uint64_t r2 = xs64();
		struct tkey k;
		cluster_key(&k, fam, base_v + (uint64_t)i * 0x9e3779b9ULL);
		op_insert_key(&k, r2 & ~0x18000ULL);	/* always apply */
	}
}

static void op_delete(uint64_t r)
{
	struct tkey k;
	struct assoc_array_edit *edit;

	if (!(r & 0x40000) || !live_key(&k, r))
		gen_key(&k, r >> 4);
	edit = assoc_array_delete(&array, &tops, &k);
	if (!edit) {
		fold(NILX);
		return;
	}
	if (IS_ERR(edit)) {
		fold((uint64_t)PTR_ERR(edit));
		return;
	}
	fold(0xde1);
	fold_edit(edit);
	if (r & 0x20000) {
		fold(0xca9);
		assoc_array_cancel_edit(edit);
	} else {
		assoc_array_apply_edit(edit);
	}
}

static void op_find(uint64_t r)
{
	struct tkey k;
	void *obj;

	if (!(r & 0x40000) || !live_key(&k, r))
		gen_key(&k, r >> 4);
	obj = assoc_array_find(&array, &tops, &k);
	fold(canon(assoc_array_leaf_to_ptr(obj)));
}

struct iter_ctx {
	int stop_after;
	int n;
};

static int iter_cb(const void *object, void *iterator_data)
{
	struct iter_ctx *c = iterator_data;

	fold(0x17e7);
	fold(canon(assoc_array_leaf_to_ptr(object)));
	c->n++;
	return c->n >= c->stop_after ? 13 + c->n : 0;
}

static void op_iterate(uint64_t r)
{
	struct iter_ctx c;

	c.stop_after = (r & 0x8000) ? 1 + (int)((r >> 16) % 5) : 1000000;
	c.n = 0;
	fold(0x17aa);
	fold((uint64_t)(int64_t)assoc_array_iterate(&array, iter_cb, &c));
	fold((uint64_t)c.n);
}

static void op_clear(uint64_t r)
{
	struct assoc_array_edit *edit;

	edit = assoc_array_clear(&array, &tops);
	if (!edit) {
		fold(NILX);
		return;
	}
	if (IS_ERR(edit)) {
		fold((uint64_t)PTR_ERR(edit));
		return;
	}
	fold(0xc1ea);
	fold_edit(edit);
	if (r & 0x20000) {
		fold(0xca9);
		assoc_array_cancel_edit(edit);
	} else {
		assoc_array_apply_edit(edit);
	}
}

struct gc_ctx {
	uint64_t mode;
};

static bool gc_cb(void *object, void *iterator_data)
{
	struct gc_ctx *c = iterator_data;
	int i = obj_idx(object);
	int keep;

	switch (c->mode % 4) {
	case 0:
		keep = 1;				/* keep all */
		break;
	case 1:
		keep = i & 1;				/* odd slots */
		break;
	case 2:
		keep = (int)(opool[i].key.w[0] & 1);	/* key bit 0 */
		break;
	default:
		keep = (i % 4) != 0;			/* most */
		break;
	}
	fold(keep);
	if (keep)
		opool[i].refs++;	/* iterator must ref what it keeps */
	return keep;
}

static void op_gc(uint64_t r)
{
	struct gc_ctx c;

	c.mode = r >> 8;
	fold(0x9c);
	fold((uint64_t)(int64_t)assoc_array_gc(&array, &tops, gc_cb, &c));
}

static void op_destroy(uint64_t r)
{
	fold(0xde57);
	assoc_array_destroy(&array, &tops);
	fold(canon(array.root));
	fold(array.nr_leaves_on_tree);
	if (r & 0x80000)
		assoc_array_init(&array);	/* also reset the counter */
	fold(array.nr_leaves_on_tree);
}

static void op_misc(uint64_t r)
{
	struct tkey k;
	void *obj;

	switch (r % 4) {
	case 0:
		fold(array.nr_leaves_on_tree);
		break;
	case 1: {	/* iterate counting, no early stop */
		struct iter_ctx c = { 1000000, 0 };
		fold((uint64_t)(int64_t)assoc_array_iterate(&array,
							  iter_cb, &c));
		break;
	}
	case 2:		/* find then delete the same key */
		gen_key(&k, r >> 8);
		obj = assoc_array_find(&array, &tops, &k);
		fold(canon(assoc_array_leaf_to_ptr(obj)));
		if (obj) {
			struct assoc_array_edit *e =
				assoc_array_delete(&array, &tops, &k);
			fold(e && !IS_ERR(e));
			if (e && !IS_ERR(e)) {
				fold_edit(e);
				assoc_array_apply_edit(e);
			}
		}
		break;
	default: {	/* a second insert, always applied */
		struct tkey k2;
		gen_key(&k2, r >> 8);
		op_insert_key(&k2, (r >> 8) & ~0x18000ULL);
		break;
	}
	}
}

uint64_t wrap_assoc_seq(uint64_t seed, int64_t nops_i, int64_t seed2)
{
	int nops = (int)(iabs(nops_i) % 190);
	int dump_every = 5 + (int)(iabs(seed2) % 7);
	int i;

	reset_all();
	rng_state = seed | 1;
	fail_mask = iabs(seed2) ? iabs(seed2) * 0x9e3779b97f4a7c15ULL : 0;
	if (iabs(seed2) % 5 == 1)
		fail_mask = ~0ULL;

	for (i = 0; i < nops; i++) {
		uint64_t r = xs64();
		if (trace_on())
			fprintf(stderr, "OP %d %016llx\n", i,
				(unsigned long long)r);
		switch ((int)(r % 16)) {
		case 0: case 1:
			op_insert(r);
			break;
		case 2: case 3:
			op_cluster(r);
			break;
		case 4:
			op_insert_burst(r);
			break;
		case 5: case 6: case 7:
			op_delete(r);
			break;
		case 8: case 9:
			op_find(r);
			break;
		case 10:
			op_iterate(r);
			break;
		case 11:
			op_gc(r);
			break;
		case 12:
			op_clear(r);
			break;
		case 13:
			op_destroy(r);
			break;
		case 14:
			op_misc(r);
			break;
		default:
			op_insert(r);
			break;
		}
		if (i % dump_every == dump_every - 1)
			dump_state();
	}
	dump_state();
	fold(alloc_ctr);
	return g_h;
}
