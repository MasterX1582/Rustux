/* Scalar wrappers around lib/rbtree.c and the
 * include/linux/{rbtree,rbtree_types,rbtree_augmented}.h inline layer,
 * so the standard scalar bridge can drive them.
 *
 * rb_node/rb_root are pointer structures the bridge cannot express, so
 * every tree lives in a static pool of N=64 nodes; node identity is the
 * pool index (0..63) and NULL is folded into checksums as 0xffff.
 *
 * Payloads are reached through rb_entry()/container_of() exactly like
 * real users (mm/vma, sched): struct tnode wraps rb_node with a key,
 * a value and an augmented field; struct ltnode wraps rb_node_linked.
 * Keys are drawn from [0, 3*N) so duplicates are common; a duplicate
 * insert takes the right branch at the equal node (upstream leaves the
 * placement of equal keys unspecified — the "right on equal" rule is
 * the deterministic convention used here and documented for the port).
 *
 * Every wrapper returns one order-sensitive FNV-1a-style checksum:
 *   h = (h ^ v) * 0x100000001b3
 * folded over each op's result (inserted/erased/found index, query
 * return values, replace pairs) and finally over a full in-order walk
 * emitting (index, key, color, parent, left, right [, aug]) per node.
 * Raw pointers are never folded, so the checksum is address-independent.
 */
#include <stdint.h>
#include <stddef.h>
#include <string.h>
#include <linux/types.h>
#include <linux/rbtree_augmented.h>

#define N 64
#define NILX 0xffffu
#define KEYSPACE (3u * N)

struct tnode {
	struct rb_node rb;
	uint64_t key;
	uint64_t val;
	uint64_t aug;
	int present;
};

struct ltnode {
	struct rb_node_linked ln;
	uint64_t key;
	int present;
};

static struct tnode pool[N];
static struct ltnode lpool[N];
static struct rb_root root = RB_ROOT;
static struct rb_root_cached croot = RB_ROOT_CACHED;
static struct rb_root_linked lroot = RB_ROOT_LINKED;

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

/* ---- container_of plumbing (same idiom as kernel users) ---- */

static struct tnode *tn(const struct rb_node *p)
{
	return rb_entry(p, struct tnode, rb);
}
static int tidx(const struct rb_node *p)
{
	return p ? (int)(tn(p) - pool) : NILX;
}
static struct ltnode *ltn(const struct rb_node_linked *l)
{
	return container_of(l, struct ltnode, ln);
}
static int lnidx(const struct rb_node_linked *l)
{
	return l ? (int)(ltn(l) - lpool) : NILX;
}
static struct ltnode *ltn2(const struct rb_node *p)
{
	return ltn(rb_entry(p, struct rb_node_linked, node));
}
static int ltidx(const struct rb_node *p)
{
	return p ? (int)(ltn2(p) - lpool) : NILX;
}

/* ---- comparators / less ops (kernel-style signatures) ---- */

static int cmp_key(const void *k, const struct rb_node *n)
{
	uint64_t a = (uint64_t)(uintptr_t)k, b = tn(n)->key;
	return (a > b) - (a < b);
}
static int cmp_node(struct rb_node *a, const struct rb_node *b)
{
	uint64_t x = tn(a)->key, y = tn(b)->key;
	return (x > y) - (x < y);
}
static bool tless(struct rb_node *a, const struct rb_node *b)
{
	return tn(a)->key < tn(b)->key;
}
static bool lless(struct rb_node *a, const struct rb_node *b)
{
	return ltn2(a)->key < ltn2(b)->key;
}

/* ---- shared helpers ---- */

static void reset_all(void)
{
	memset(pool, 0, sizeof pool);
	memset(lpool, 0, sizeof lpool);
	root = RB_ROOT;
	croot = RB_ROOT_CACHED;
	lroot = RB_ROOT_LINKED;
	g_h = 0x9e3779b97f4a7c15ULL;
}

static int tree_count_r(struct rb_root *r)
{
	struct rb_node *p = rb_first(r);
	int c = 0;
	while (p) { c++; p = rb_next(p); }
	return c;
}
static int tree_kth_r(struct rb_root *r, int k)
{
	struct rb_node *p = rb_first(r);
	while (p && k--)
		p = rb_next(p);
	return tidx(p);
}
static int free_slot(void)
{
	int i;
	for (i = 0; i < N; i++)
		if (!pool[i].present)
			return i;
	return -1;
}

/* user-side search+insert: every rbtree user implements its own;
 * duplicates go right at the equal node. */
static void tree_insert(struct rb_root *r, int i)
{
	struct rb_node **link = &r->rb_node, *parent = NULL;
	uint64_t k = pool[i].key;

	while (*link) {
		parent = *link;
		if (k < tn(parent)->key)
			link = &parent->rb_left;
		else
			link = &parent->rb_right;
	}
	pool[i].present = 1;
	rb_link_node(&pool[i].rb, parent, link);
	rb_insert_color(&pool[i].rb, r);
}

/* full state checksum: in-order walk of (idx, key, color, parent, left,
 * right [, aug]) then the node count. */
static void fold_tree(struct rb_root *r, int with_aug)
{
	struct rb_node *p = rb_first(r);
	int c = 0;
	while (p) {
		struct tnode *t = tn(p);
		fold(tidx(p));
		fold(t->key);
		fold(rb_is_black(p));
		fold(tidx(rb_parent(p)));
		fold(tidx(p->rb_left));
		fold(tidx(p->rb_right));
		if (with_aug)
			fold(t->aug);
		c++;
		p = rb_next(p);
	}
	fold(c);
}

static void fold_ltree(struct rb_root *r)
{
	struct rb_node *p = rb_first(r);
	int c = 0;
	while (p) {
		struct ltnode *t = ltn2(p);
		fold(ltidx(p));
		fold(t->key);
		fold(rb_is_black(p));
		fold(ltidx(rb_parent(p)));
		fold(ltidx(p->rb_left));
		fold(ltidx(p->rb_right));
		c++;
		p = rb_next(p);
	}
	fold(c);
}

/* victim pick for erases: odd -> random present slot, even -> kth
 * in-order node; -1 when the tree is empty / slot absent. */
static int pick_victim(struct rb_root *r, uint64_t r2)
{
	if (r2 & 1) {
		int idx = (int)((r2 >> 8) % N);
		return pool[idx].present ? idx : -1;
	} else {
		int c = tree_count_r(r);
		return c ? tree_kth_r(r, (int)((r2 >> 8) % c)) : -1;
	}
}

/* ================= plain rb_root op-mix driver ================= */

uint64_t wrap_rbtree_run(uint64_t seed, int64_t nops_i, int64_t seed2)
{
	static const int mix[4][3] = {
		{55, 30, 15}, {75, 15, 10}, {40, 40, 20}, {30, 50, 20}
	};
	const int *m = mix[iabs(seed2) % 4];
	int nops = (int)(iabs(nops_i) % 200);
	int i;

	reset_all();
	rng_state = seed | 1;

	for (i = 0; i < nops; i++) {
		uint64_t r = xs64();
		int t = (int)(r % 100);
		if (t < m[0]) {
			int idx = (int)(xs64() % N);
			if (!pool[idx].present) {
				pool[idx].key = xs64() % KEYSPACE;
				pool[idx].val = pool[idx].key;
				tree_insert(&root, idx);
				fold(idx);
			} else
				fold(NILX);
		} else if (t < m[0] + m[1]) {
			int idx = pick_victim(&root, xs64());
			if (idx >= 0) {
				rb_erase(&pool[idx].rb, &root);
				pool[idx].present = 0;
				RB_CLEAR_NODE(&pool[idx].rb);
				fold(idx);
				fold(RB_EMPTY_NODE(&pool[idx].rb));
			} else
				fold(NILX);
		} else {
			uint64_t q = xs64() % 10;
			switch (q) {
			case 0:
				fold(tidx(rb_first(&root)));
				break;
			case 1:
				fold(tidx(rb_last(&root)));
				break;
			case 2: {
				int c = tree_count_r(&root);
				if (c) {
					int k = (int)(xs64() % c);
					fold(tidx(rb_next(&pool[tree_kth_r(&root, k)].rb)));
				} else
					fold(NILX);
				break;
			}
			case 3: {
				int c = tree_count_r(&root);
				if (c) {
					int k = (int)(xs64() % c);
					fold(tidx(rb_prev(&pool[tree_kth_r(&root, k)].rb)));
				} else
					fold(NILX);
				break;
			}
			case 4: {
				struct rb_node *p = rb_first_postorder(&root);
				while (p) {
					fold(tidx(p));
					p = rb_next_postorder(p);
				}
				fold(NILX);
				break;
			}
			case 5:
			case 6: {
				int c = tree_count_r(&root);
				if (c) {
					int v = tree_kth_r(&root, (int)(xs64() % c));
					int ni = free_slot();
					if (ni < 0) {
						fold(NILX);
						break;
					}
					pool[ni].key = pool[v].key;
					pool[ni].val = pool[v].val;
					if (q == 5)
						rb_replace_node(&pool[v].rb,
								&pool[ni].rb, &root);
					else
						rb_replace_node_rcu(&pool[v].rb,
								    &pool[ni].rb, &root);
					pool[v].present = 0;
					pool[ni].present = 1;
					fold(v);
					fold(ni);
				} else
					fold(NILX);
				break;
			}
			case 7: {
				uint64_t k = xs64() % KEYSPACE;
				fold(tidx(rb_find((const void *)(uintptr_t)k,
						  &root, cmp_key)));
				break;
			}
			case 8: {
				uint64_t k = xs64() % KEYSPACE;
				struct rb_node *p;
				int j = 0;
				rb_for_each(p, (const void *)(uintptr_t)k,
					    &root, cmp_key) {
					fold(tidx(p));
					if (++j == 4)
						break;
				}
				fold(NILX);
				break;
			}
			default: {
				int ni = free_slot();
				struct rb_node *res;
				if (ni < 0) {
					fold(NILX);
					break;
				}
				pool[ni].key = xs64() % KEYSPACE;
				pool[ni].val = pool[ni].key;
				res = rb_find_add(&pool[ni].rb, &root, cmp_node);
				if (res)
					fold(tidx(res));
				else {
					pool[ni].present = 1;
					fold(ni);
				}
				break;
			}
			}
		}
	}
	fold_tree(&root, 0);
	return g_h;
}

/* ============== pure insert sequence, then queries ============== */

uint64_t wrap_rbtree_insert_seq(uint64_t seed, int64_t n_i)
{
	int n = (int)(iabs(n_i) % (N + 1));
	int i;

	reset_all();
	rng_state = seed | 1;

	for (i = 0; i < n; i++) {
		pool[i].key = xs64() % KEYSPACE;
		pool[i].val = pool[i].key;
		tree_insert(&root, i);
	}
	fold(tidx(rb_first(&root)));
	fold(tidx(rb_last(&root)));
	fold_tree(&root, 0);
	return g_h;
}

/* ============== insert n, then erase all three ways ============== */

uint64_t wrap_rbtree_erase_all(uint64_t seed, int64_t n_i)
{
	int n = (int)(iabs(n_i) % (N + 1));
	int i, mode;

	reset_all();
	rng_state = seed | 1;

	for (i = 0; i < n; i++) {
		pool[i].key = xs64() % KEYSPACE;
		pool[i].val = pool[i].key;
		tree_insert(&root, i);
	}
	mode = (int)(xs64() % 3);
	if (mode == 0) {
		struct rb_node *p;
		while ((p = rb_first(&root))) {
			int idx = tidx(p);
			rb_erase(p, &root);
			pool[idx].present = 0;
			fold(idx);
		}
	} else if (mode == 1) {
		struct rb_node *p;
		while ((p = rb_last(&root))) {
			int idx = tidx(p);
			rb_erase(p, &root);
			pool[idx].present = 0;
			fold(idx);
		}
	} else {
		for (i = 0; i < N; i++) {
			if (pool[i].present) {
				rb_erase(&pool[i].rb, &root);
				pool[i].present = 0;
				fold(i);
			}
		}
	}
	fold_tree(&root, 0);
	return g_h;
}

/* ============== rb_find/rb_find_first/rb_next_match/rb_find_add ==== */

uint64_t wrap_rbtree_find(uint64_t seed, int64_t n_i, int64_t seed2)
{
	int n = (int)(iabs(n_i) % (N / 2));
	int rcu_j = (int)(iabs(seed2) % 8);
	int i, j, ni;

	reset_all();
	rng_state = seed | 1;

	for (i = 0; i < n; i++) {
		pool[i].key = xs64() % KEYSPACE;
		pool[i].val = pool[i].key;
		tree_insert(&root, i);
	}
	for (j = 0; j < 12; j++) {
		uint64_t k = xs64() % KEYSPACE;
		struct rb_node *p;
		int c = 0;
		fold(tidx(rb_find((const void *)(uintptr_t)k, &root, cmp_key)));
		fold(tidx(rb_find_rcu((const void *)(uintptr_t)k, &root,
				      cmp_key)));
		rb_for_each(p, (const void *)(uintptr_t)k, &root, cmp_key) {
			fold(tidx(p));
			if (++c == 4)
				break;
		}
		fold(NILX);
	}
	ni = n;
	for (j = 0; j < 8 && ni < N; j++, ni++) {
		struct rb_node *res;
		pool[ni].key = xs64() % KEYSPACE;
		pool[ni].val = pool[ni].key;
		if (j == rcu_j)
			res = rb_find_add_rcu(&pool[ni].rb, &root, cmp_node);
		else
			res = rb_find_add(&pool[ni].rb, &root, cmp_node);
		if (res)
			fold(tidx(res));
		else {
			pool[ni].present = 1;
			fold(ni);
		}
	}
	for (j = 0; j < 4 && ni < N; j++, ni++) {
		pool[ni].key = xs64() % KEYSPACE;
		pool[ni].val = pool[ni].key;
		pool[ni].present = 1;
		rb_add(&pool[ni].rb, &root, tless);
		fold(ni);
	}
	fold_tree(&root, 0);
	return g_h;
}

/* ================= rb_root_cached driver ================= */

uint64_t wrap_rbtree_cached(uint64_t seed, int64_t nops_i)
{
	int nops = (int)(iabs(nops_i) % 200);
	int i;

	reset_all();
	rng_state = seed | 1;

	for (i = 0; i < nops; i++) {
		uint64_t r = xs64();
		int t = (int)(r % 100);
		if (t < 55) {
			int idx = (int)(xs64() % N);
			if (!pool[idx].present) {
				pool[idx].key = xs64() % KEYSPACE;
				pool[idx].val = pool[idx].key;
				if (r & 0x200) {
					struct rb_node *res =
						rb_add_cached(&pool[idx].rb,
							      &croot, tless);
					fold(tidx(res));
				} else {
					struct rb_node **link =
						&croot.rb_root.rb_node;
					struct rb_node *parent = NULL;
					uint64_t k = pool[idx].key;
					bool leftmost = true;
					while (*link) {
						parent = *link;
						if (k < tn(parent)->key)
							link = &parent->rb_left;
						else {
							link = &parent->rb_right;
							leftmost = false;
						}
					}
					rb_link_node(&pool[idx].rb, parent,
						     link);
					rb_insert_color_cached(&pool[idx].rb,
							       &croot, leftmost);
					fold(leftmost);
				}
				pool[idx].present = 1;
			} else
				fold(NILX);
		} else if (t < 85) {
			int idx = pick_victim(&croot.rb_root, xs64());
			if (idx >= 0) {
				struct rb_node *lm =
					rb_erase_cached(&pool[idx].rb, &croot);
				pool[idx].present = 0;
				fold(idx);
				fold(tidx(lm));
			} else
				fold(NILX);
		} else {
			uint64_t q = xs64() % 6;
			switch (q) {
			case 0:
				fold(tidx(rb_first_cached(&croot)));
				break;
			case 1:
				fold(tidx(rb_first(&croot.rb_root)));
				break;
			case 2:
				fold(tidx(rb_first_cached(&croot)) ==
				     tidx(rb_first(&croot.rb_root)));
				break;
			case 3: {
				int c = tree_count_r(&croot.rb_root);
				if (c) {
					int v = tree_kth_r(&croot.rb_root,
							   (int)(xs64() % c));
					int ni = free_slot();
					if (ni < 0) {
						fold(NILX);
						break;
					}
					pool[ni].key = pool[v].key;
					pool[ni].val = pool[v].val;
					rb_replace_node_cached(&pool[v].rb,
							       &pool[ni].rb,
							       &croot);
					pool[v].present = 0;
					pool[ni].present = 1;
					fold(v);
					fold(ni);
				} else
					fold(NILX);
				break;
			}
			case 4: {
				int ni = free_slot();
				struct rb_node *res;
				if (ni < 0) {
					fold(NILX);
					break;
				}
				pool[ni].key = xs64() % KEYSPACE;
				pool[ni].val = pool[ni].key;
				res = rb_find_add_cached(&pool[ni].rb, &croot,
							 cmp_node);
				if (res)
					fold(tidx(res));
				else {
					pool[ni].present = 1;
					fold(ni);
				}
				break;
			}
			default:
				fold(tidx(rb_last(&croot.rb_root)));
				break;
			}
		}
	}
	fold(tidx(croot.rb_leftmost));
	fold_tree(&croot.rb_root, 0);
	return g_h;
}

/* ================= rb_root_linked driver ================= */

static int llist_count(void)
{
	struct rb_node_linked *l = lroot.rb_leftmost;
	int c = 0;
	while (l && c <= N) { c++; l = l->next; }
	return c;
}

uint64_t wrap_rbtree_linked(uint64_t seed, int64_t nops_i)
{
	int nops = (int)(iabs(nops_i) % 200);
	int i;

	reset_all();
	rng_state = seed | 1;

	for (i = 0; i < nops; i++) {
		uint64_t r = xs64();
		int t = (int)(r % 100);
		if (t < 60) {
			int idx = (int)(xs64() % N);
			if (!lpool[idx].present) {
				bool lm;
				lpool[idx].key = xs64() % KEYSPACE;
				lm = rb_add_linked(&lpool[idx].ln, &lroot,
						   lless);
				lpool[idx].present = 1;
				fold(idx);
				fold(lm);
			} else
				fold(NILX);
		} else if (t < 85) {
			uint64_t r2 = xs64();
			int idx = -1;
			if (r2 & 1) {
				int s = (int)((r2 >> 8) % N);
				if (lpool[s].present)
					idx = s;
			} else {
				int c = llist_count();
				if (c) {
					int k = (int)((r2 >> 8) % c);
					struct rb_node_linked *l =
						lroot.rb_leftmost;
					while (l && k--)
						l = l->next;
					idx = lnidx(l);
				}
			}
			if (idx >= 0) {
				bool more = rb_erase_linked(&lpool[idx].ln,
							    &lroot);
				lpool[idx].present = 0;
				fold(idx);
				fold(more);
				fold(RB_EMPTY_LINKED_NODE(&lpool[idx].ln));
			} else
				fold(NILX);
		} else {
			uint64_t q = xs64() % 4;
			switch (q) {
			case 0:
				fold(lnidx(lroot.rb_leftmost));
				break;
			case 1:
				fold(ltidx(rb_first(&lroot.rb_root)));
				break;
			case 2: {
				struct rb_node_linked *l = lroot.rb_leftmost;
				int j = 0;
				while (l && j < 8) {
					fold(lnidx(l));
					fold(ltn(l)->key);
					l = l->next;
					j++;
				}
				fold(NILX);
				break;
			}
			default: {
				int c = tree_count_r(&lroot.rb_root);
				if (c) {
					int k = (int)(xs64() % c);
					struct rb_node *p =
						rb_first(&lroot.rb_root);
					while (p && k--)
						p = rb_next(p);
					fold(ltidx(p));
				} else
					fold(NILX);
				break;
			}
			}
		}
	}
	fold(lnidx(lroot.rb_leftmost));
	{
		struct rb_node_linked *l = lroot.rb_leftmost;
		while (l) {
			fold(lnidx(l));
			fold(ltn(l)->key);
			l = l->next;
		}
		fold(NILX);
	}
	fold_ltree(&lroot.rb_root);
	return g_h;
}

/* ================= augmented tree driver ================= */

#define TNODE_VAL(n) ((n)->val)
RB_DECLARE_CALLBACKS_MAX(static, aug_cb, struct tnode, rb,
			 uint64_t, aug, TNODE_VAL)

/* manual descent insert, rbtree_test.c idiom: fold the new node's
 * value into every ancestor's aug on the way down, then
 * rb_insert_augmented_cached() lets ->rotate fix the rest. */
static void aug_insert(int i)
{
	struct rb_node **link = &croot.rb_root.rb_node, *parent = NULL;
	uint64_t k = pool[i].key, v = pool[i].val;
	bool leftmost = true;
	struct tnode *p;

	while (*link) {
		parent = *link;
		p = tn(parent);
		if (p->aug < v)
			p->aug = v;
		if (k < p->key)
			link = &parent->rb_left;
		else {
			link = &parent->rb_right;
			leftmost = false;
		}
	}
	pool[i].aug = v;
	rb_link_node(&pool[i].rb, parent, link);
	rb_insert_augmented_cached(&pool[i].rb, &croot, leftmost, &aug_cb);
	fold(leftmost);
}

uint64_t wrap_rbtree_augmented(uint64_t seed, int64_t nops_i)
{
	int nops = (int)(iabs(nops_i) % 200);
	int i;

	reset_all();
	rng_state = seed | 1;

	for (i = 0; i < nops; i++) {
		uint64_t r = xs64();
		int t = (int)(r % 100);
		if (t < 55) {
			int idx = (int)(xs64() % N);
			if (!pool[idx].present) {
				pool[idx].key = xs64() % KEYSPACE;
				pool[idx].val = xs64() % KEYSPACE;
				if (r & 0x200)
					aug_insert(idx);
				else {
					struct rb_node *res;
					pool[idx].aug = pool[idx].val;
					res = rb_add_augmented_cached(
						&pool[idx].rb, &croot,
						tless, &aug_cb);
					fold(tidx(res));
				}
				pool[idx].present = 1;
			} else
				fold(NILX);
		} else if (t < 85) {
			int idx = pick_victim(&croot.rb_root, xs64());
			if (idx >= 0) {
				rb_erase_augmented_cached(&pool[idx].rb,
							  &croot, &aug_cb);
				pool[idx].present = 0;
				fold(idx);
			} else
				fold(NILX);
		} else {
			uint64_t q = xs64() % 3;
			switch (q) {
			case 0:
				fold(tidx(rb_first_cached(&croot)));
				break;
			case 1:
				if (croot.rb_root.rb_node)
					fold(tn(croot.rb_root.rb_node)->aug);
				else
					fold(NILX);
				break;
			default:
				fold(tidx(croot.rb_leftmost));
				break;
			}
		}
	}
	fold(tidx(croot.rb_leftmost));
	fold_tree(&croot.rb_root, 1);
	return g_h;
}

/* ================= postorder traversal ================= */

uint64_t wrap_rbtree_postorder(uint64_t seed, int64_t n_i)
{
	int n = (int)(iabs(n_i) % (N + 1));
	int i;
	struct rb_node *p;

	reset_all();
	rng_state = seed | 1;

	for (i = 0; i < n; i++) {
		pool[i].key = xs64() % KEYSPACE;
		pool[i].val = pool[i].key;
		tree_insert(&root, i);
	}
	p = rb_first_postorder(&root);
	fold(tidx(p));
	while (p) {
		p = rb_next_postorder(p);
		fold(tidx(p));
	}
	fold(tidx(rb_next_postorder(NULL)));
	fold_tree(&root, 0);
	return g_h;
}

/* ================= rb_replace_node / _rcu sequence ================= */

uint64_t wrap_rb_replace(uint64_t seed, int64_t n_i, int64_t k_i)
{
	int n = (int)(iabs(n_i) % (N / 2 + 1));
	int m = (int)(iabs(k_i) % 8);
	int i, j;

	reset_all();
	rng_state = seed | 1;

	for (i = 0; i < n; i++) {
		pool[i].key = xs64() % KEYSPACE;
		pool[i].val = pool[i].key;
		tree_insert(&root, i);
	}
	for (j = 0; j < m; j++) {
		int c = tree_count_r(&root);
		int v, ni;
		if (!c)
			break;
		v = tree_kth_r(&root, (int)(xs64() % c));
		ni = free_slot();
		if (ni < 0)
			break;
		pool[ni].key = pool[v].key;
		pool[ni].val = pool[v].val;
		if (j & 1)
			rb_replace_node_rcu(&pool[v].rb, &pool[ni].rb, &root);
		else
			rb_replace_node(&pool[v].rb, &pool[ni].rb, &root);
		pool[v].present = 0;
		pool[ni].present = 1;
		fold(v);
		fold(ni);
	}
	fold_tree(&root, 0);
	return g_h;
}
