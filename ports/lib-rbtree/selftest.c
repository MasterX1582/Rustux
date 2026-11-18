/* Invariant self-test (not part of the shipped port): drives the same
 * op mix as wrappers.c but validates red-black properties, augmented
 * fields and the linked list after every op. Exit 0 = all good. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <linux/types.h>
#include <linux/rbtree_augmented.h>

#define N 64
#define KEYSPACE (3u * N)

struct tnode { struct rb_node rb; uint64_t key, val, aug; int present; };
struct ltnode { struct rb_node_linked ln; uint64_t key; int present; };

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
static struct tnode *tn(const struct rb_node *p)
{ return rb_entry(p, struct tnode, rb); }
static int tidx(const struct rb_node *p)
{ return p ? (int)(tn(p) - pool) : -1; }
static struct ltnode *ltn(const struct rb_node_linked *l)
{ return container_of(l, struct ltnode, ln); }
static struct ltnode *ltn2(const struct rb_node *p)
{ return ltn(rb_entry(p, struct rb_node_linked, node)); }

static int fails;
#define CHECK(c, ...) do { if (!(c)) { fails++; \
	printf("FAIL %d: ", __LINE__); printf(__VA_ARGS__); \
	printf("\n"); if (fails > 20) exit(1); } } while (0)

/* validate whole tree; returns black-height, fills n. */
static int validate(struct rb_node *p, struct rb_node *par, int *n,
		    int aug)
{
	int bh_l, bh_r;
	if (!p)
		return 1;
	if (rb_parent(p) != par) {
		CHECK(0, "bad parent of %d", tidx(p));
		return 0;
	}
	if (rb_is_red(p)) {
		CHECK(!par || rb_is_black(par), "red-red at %d", tidx(p));
		CHECK(!p->rb_left || rb_is_black(p->rb_left), "red-red l");
		CHECK(!p->rb_right || rb_is_black(p->rb_right), "red-red r");
	}
	bh_l = validate(p->rb_left, p, n, aug);
	bh_r = validate(p->rb_right, p, n, aug);
	CHECK(bh_l == bh_r, "black-height mismatch at %d", tidx(p));
	if (aug) {
		struct tnode *t = tn(p);
		uint64_t want = t->val;
		if (p->rb_left && tn(p->rb_left)->aug > want)
			want = tn(p->rb_left)->aug;
		if (p->rb_right && tn(p->rb_right)->aug > want)
			want = tn(p->rb_right)->aug;
		CHECK(t->aug == want, "aug of %d = %lu want %lu",
		      tidx(p), t->aug, want);
	}
	(*n)++;
	return bh_l + rb_is_black(p);
}

static void check_tree(struct rb_root *r, int aug)
{
	int n = 0;
	if (r->rb_node) {
		CHECK(rb_is_black(r->rb_node), "root not black");
		validate(r->rb_node, NULL, &n, aug);
	}
}

static struct rb_node *rb_last_sub(struct rb_node *p)
{
	while (p->rb_right) p = p->rb_right;
	return p;
}

static void check_sorted(struct rb_root *r)
{
	struct rb_node *p = rb_first(r);
	uint64_t prev = 0;
	int first = 1, cnt = 0;
	while (p) {
		if (!first)
			CHECK(tn(p)->key >= prev, "in-order not sorted");
		prev = tn(p)->key;
		first = 0;
		cnt++;
		CHECK(p->rb_left == NULL ||
		      tn(rb_last_sub(p->rb_left))->key <= tn(p)->key,
		      "left subtree key too big");
		p = rb_next(p);
	}
}

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

#define TNODE_VAL(n) ((n)->val)
RB_DECLARE_CALLBACKS_MAX(static, aug_cb, struct tnode, rb,
			 uint64_t, aug, TNODE_VAL)

static bool tless(struct rb_node *a, const struct rb_node *b)
{ return tn(a)->key < tn(b)->key; }
static bool lless(struct rb_node *a, const struct rb_node *b)
{ return ltn2(a)->key < ltn2(b)->key; }

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
}

int main(void)
{
	int seed, i, t;
	for (seed = 1; seed <= 40; seed++) {
		memset(pool, 0, sizeof pool);
		root = RB_ROOT;
		rng_state = seed;
		/* random insert/erase mix, validate every op */
		for (i = 0; i < 400; i++) {
			uint64_t r = xs64();
			int idx = (int)(xs64() % N);
			if (r % 100 < 60) {
				if (!pool[idx].present) {
					pool[idx].key = xs64() % KEYSPACE;
					tree_insert(&root, idx);
				}
			} else {
				if (pool[idx].present) {
					rb_erase(&pool[idx].rb, &root);
					pool[idx].present = 0;
				}
			}
		}
		check_tree(&root, 0);
		check_sorted(&root);
		/* erase everything in random order, validate each step */
		for (t = 0; t < 64; t++) {
			for (i = 0; i < N; i++)
				if (pool[i].present && xs64() % 2) {
					rb_erase(&pool[i].rb, &root);
					pool[i].present = 0;
					check_tree(&root, 0);
				}
		}
		check_tree(&root, 0);
		CHECK(root.rb_node == NULL, "not empty after erasing all");

		/* augmented */
		memset(pool, 0, sizeof pool);
		croot = RB_ROOT_CACHED;
		for (i = 0; i < 200; i++) {
			uint64_t r = xs64();
			int idx = (int)(xs64() % N);
			if (r % 100 < 60) {
				if (!pool[idx].present) {
					pool[idx].key = xs64() % KEYSPACE;
					pool[idx].val = xs64() % KEYSPACE;
					aug_insert(idx);
					pool[idx].present = 1;
				}
			} else if (pool[idx].present) {
				rb_erase_augmented_cached(&pool[idx].rb,
							  &croot, &aug_cb);
				pool[idx].present = 0;
			}
			check_tree(&croot.rb_root, 1);
			{
				struct rb_node *f = rb_first(&croot.rb_root);
				CHECK(tidx(f) == tidx(croot.rb_leftmost),
				      "cached leftmost wrong");
			}
		}

		/* linked */
		memset(lpool, 0, sizeof lpool);
		lroot = RB_ROOT_LINKED;
		for (i = 0; i < 200; i++) {
			uint64_t r = xs64();
			int idx = (int)(xs64() % N);
			if (r % 100 < 60) {
				if (!lpool[idx].present) {
					lpool[idx].key = xs64() % KEYSPACE;
					rb_add_linked(&lpool[idx].ln, &lroot,
						      lless);
					lpool[idx].present = 1;
				}
			} else if (lpool[idx].present) {
				rb_erase_linked(&lpool[idx].ln, &lroot);
				lpool[idx].present = 0;
			}
			/* list must be sorted & match in-order walk */
			{
				struct rb_node_linked *l = lroot.rb_leftmost;
				struct rb_node *p = rb_first(&lroot.rb_root);
				int cnt = 0;
				while (l && p) {
					CHECK(ltn(l) == ltn2(p),
					      "list/tree order mismatch");
					l = l->next;
					p = rb_next(p);
					cnt++;
				}
				CHECK(l == NULL && p == NULL,
				      "list/tree length mismatch");
				(void)cnt;
			}
		}
	}
	printf("%s (%d failures)\n", fails ? "FAILURES" : "ALL OK", fails);
	return fails ? 1 : 0;
}
