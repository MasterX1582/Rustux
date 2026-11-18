/* Scalar wrappers around lib/llist.c and the include/linux/llist.h
 * inline layer, so the standard scalar bridge can drive them.
 *
 * struct llist_node * is a pointer the bridge cannot express, so the
 * list lives in a static pool of N=32 nodes; node identity is the pool
 * index (0..31) and NULL is folded into checksums as 0xffff (NILX).
 *
 * Payloads are reached through llist_entry()/container_of() exactly
 * like real users (e.g. kvm, io_uring): struct tnode wraps llist_node
 * with a value and an on-list bookkeeping flag (harness-only state —
 * kernel code never reads it; adding an already-listed node or
 * deleting an unlisted one is out of contract and creates cycles, so
 * the op picker respects the flag exactly like a correct caller).
 *
 * The lockless cmpxchg ops are shimed single-threaded (see
 * linux/atomic.h): every cmpxchg succeeds on the first pass, so the
 * sequential semantics are the recorded contract.
 *
 * Every wrapper returns one order-sensitive FNV-1a-style checksum:
 *   h = (h ^ v) * 0x100000001b3
 * folded over each op's result (add/del return values, deleted node
 * indices, walk order) and finally over the whole chain.
 */
#include <stdint.h>
#include <stddef.h>
#include <string.h>
#include <linux/types.h>
#include <linux/llist.h>

#define N 32
#define NILX 0xffffu
#define TEND 0xfff0u	/* chain terminator fold value */

struct tnode {
	uint64_t val;
	struct llist_node node;	/* not first: container_of does real math */
	int on_list;
};

static struct tnode pool[N];
static struct llist_head head;

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

static int nidx(const struct llist_node *p)
{
	return p ? (int)(container_of(p, struct tnode, node) - pool) : NILX;
}

static int free_count(int npool)
{
	int i, c = 0;
	for (i = 0; i < npool; i++)
		c += !pool[i].on_list;
	return c;
}

/* k-th free slot (0-based), or -1. */
static int kth_free(int npool, int k)
{
	int i;
	for (i = 0; i < npool; i++)
		if (!pool[i].on_list && k-- == 0)
			return i;
	return -1;
}

/* fold the live chain newest-to-oldest, then a terminator. */
static void fold_chain(struct llist_node *first)
{
	struct llist_node *p;
	int c = 0;
	llist_for_each(p, first) {
		fold(nidx(p));
		fold(pool[nidx(p)].val);
		c++;
	}
	fold(TEND);
	fold(c);
}

static void fold_chain_entry(struct llist_node *first)
{
	struct tnode *t;
	int c = 0;
	/* exercises llist_entry/container_of + member_address_is_nonnull
	 * on the NULL terminator */
	llist_for_each_entry(t, first, node) {
		fold((int)(t - pool));
		fold(t->val);
		c++;
	}
	fold(TEND);
	fold(c);
}

static void fold_chain_entry_safe(struct llist_node *first)
{
	struct tnode *t, *n;
	int c = 0;
	llist_for_each_entry_safe(t, n, first, node) {
		fold((int)(t - pool));
		fold(t->val);
		fold(member_address_is_nonnull(n, node));
		c++;
	}
	fold(TEND);
	fold(c);
}

static void reset_all(void)
{
	memset(pool, 0, sizeof pool);
	init_llist_head(&head);
	g_h = 0x9e3779b97f4a7c15ULL;
}

/* ============== seeded op-mix driver ==============
 * seed   -> rng + op mix
 * npool_i -> usable pool slots, iabs % (N+1): covers 0/1/2/N edges
 * nops_i -> op count, iabs % 200
 */
uint64_t wrap_llist_seq(uint64_t seed, int64_t npool_i, int64_t nops_i)
{
	int npool = (int)(iabs(npool_i) % (N + 1));
	int nops = (int)(iabs(nops_i) % 200);
	int i;

	reset_all();
	rng_state = seed | 1;
	for (i = 0; i < npool; i++)
		pool[i].val = xs64();

	for (i = 0; i < nops; i++) {
		uint64_t r = xs64();
		int op = (int)(r % 16);

		switch (op) {
		case 0: case 1: case 2: {	/* llist_add / __llist_add */
			int idx = kth_free(npool, (int)((r >> 8) %
					(free_count(npool) | !free_count(npool))));
			if (idx < 0) {
				fold(0xbad0);
				break;
			}
			if (r & 0x80)
				fold(llist_add(&pool[idx].node, &head));
			else
				fold(__llist_add(&pool[idx].node, &head));
			pool[idx].on_list = 1;
			fold(idx);
			break;
		}
		case 3: case 4: {	/* llist_add_batch / __llist_add_batch */
			int fc = free_count(npool);
			int k = 1 + (int)((r >> 8) % 4);
			int first = -1, last = -1, prev = -1, j;
			if (k > fc)
				k = fc;
			if (!k) {
				fold(0xbad1);
				break;
			}
			for (j = 0; j < k; j++) {
				int idx = kth_free(npool, 0);
				if (j == 0)
					first = idx;
				else
					pool[prev].node.next = &pool[idx].node;
				pool[idx].on_list = 1;
				prev = idx;
				last = idx;
			}
			pool[last].node.next = NULL;
			if (r & 0x80)
				fold(llist_add_batch(&pool[first].node,
						     &pool[last].node, &head));
			else
				fold(__llist_add_batch(&pool[first].node,
						       &pool[last].node, &head));
			fold(first);
			fold(k);
			break;
		}
		case 5: case 6: {	/* llist_del_first / _init */
			struct llist_node *p;
			if (r & 0x80) {
				p = llist_del_first_init(&head);
				fold(nidx(p));
				fold(p ? llist_on_list(p) : 7);
			} else {
				p = llist_del_first(&head);
				fold(nidx(p));
			}
			if (p)
				pool[nidx(p)].on_list = 0;
			break;
		}
		case 7: {	/* llist_del_first_this */
			int idx;
			if (!llist_empty(&head) && (r & 0x80))
				idx = nidx(head.first);	/* hit */
			else if (r & 0x40)
				idx = kth_free(npool, (int)((r >> 8) %
					(free_count(npool) | !free_count(npool)))); /* absent */
			else
				idx = (int)((r >> 8) % (npool | !npool)); /* maybe mid/absent */
			if (idx < 0) {
				fold(0xbad2);
				break;
			}
			{
				bool ret = llist_del_first_this(&head,
								&pool[idx].node);
				fold(ret);
				if (ret)
					pool[idx].on_list = 0;
				else if (!pool[idx].on_list)
					fold(0xe11); /* was already off-list */
				else
					fold(0xe12); /* on list, not first */
			}
			fold(idx);
			break;
		}
		case 8: {	/* llist_del_all / __llist_del_all */
			struct llist_node *p, *q;
			int c = 0;
			if (r & 0x80)
				p = llist_del_all(&head);
			else
				p = __llist_del_all(&head);
			fold(nidx(p));
			llist_for_each_safe(p, q, p) {
				fold(nidx(p));
				pool[nidx(p)].on_list = 0;
				c++;
			}
			fold(TEND);
			fold(c);
			break;
		}
		case 9: {	/* llist_reverse_order on the detached chain */
			struct llist_node *p = __llist_del_all(&head);
			struct llist_node *q, *n2;
			fold(nidx(p));
			p = llist_reverse_order(p);
			head.first = p;
			fold(nidx(p));
			llist_for_each_safe(q, n2, p)
				fold(nidx(q));
			fold(TEND);
			break;
		}
		case 10:	/* traversal variants */
			fold(llist_empty(&head));
			switch ((r >> 8) % 3) {
			case 0: fold_chain(head.first); break;
			case 1: fold_chain_entry(head.first); break;
			case 2: fold_chain_entry_safe(head.first); break;
			}
			break;
		case 11: {	/* llist_next / peek head */
			struct llist_node *p = head.first;
			fold(nidx(p));
			fold(nidx(p ? llist_next(p) : NULL));
			break;
		}
		case 12: {	/* init_llist_node / llist_on_list */
			int idx = (int)((r >> 8) % (npool | !npool));
			fold(idx);
			fold(llist_on_list(&pool[idx].node));
			if (!pool[idx].on_list) {
				init_llist_node(&pool[idx].node);
				fold(llist_on_list(&pool[idx].node));
			}
			break;
		}
		case 13:	/* empty + head probe */
			fold(llist_empty(&head));
			fold(nidx(head.first));
			break;
		default:	/* 14,15: re-fold state */
			fold_chain(head.first);
			break;
		}
	}
	fold_chain(head.first);
	return g_h;
}

/* ============== reverse-order focus: build n, detach, reverse ============== */

uint64_t wrap_llist_reverse(uint64_t seed, int64_t n_i)
{
	int n = (int)(iabs(n_i) % (N + 1));
	struct llist_node *p;
	int i, mode;

	reset_all();
	rng_state = seed | 1;

	for (i = 0; i < n; i++)
		pool[i].val = xs64();

	mode = (int)(xs64() % 2);
	for (i = 0; i < n; i++) {
		if (mode)
			fold(llist_add(&pool[i].node, &head));
		else
			fold(__llist_add(&pool[i].node, &head));
		pool[i].on_list = 1;
	}

	p = llist_del_all(&head);
	fold(nidx(p));
	p = llist_reverse_order(p);
	head.first = p;
	fold_chain_entry(head.first);

	/* reverse again -> original order */
	p = __llist_del_all(&head);
	p = llist_reverse_order(p);
	head.first = p;
	fold_chain(head.first);
	return g_h;
}

/* ============== add_batch focus: chain of k, then chain of m ============== */

uint64_t wrap_llist_batch(uint64_t seed, int64_t k_i)
{
	int k = (int)(iabs(k_i) % (N + 1));
	int m, i, first = -1, prev = -1;
	uint64_t r;

	reset_all();
	rng_state = seed | 1;
	for (i = 0; i < N; i++)
		pool[i].val = xs64();

	/* chain 0..k-1, add in one batch (empty -> true) */
	for (i = 0; i < k; i++) {
		if (i)
			pool[prev].node.next = &pool[i].node;
		else
			first = i;
		pool[i].on_list = 1;
		prev = i;
	}
	if (k) {
		pool[k - 1].node.next = NULL;
		fold(llist_add_batch(&pool[first].node, &pool[k - 1].node,
				     &head));
	} else {
		fold(0xbad3);
	}
	fold_chain(head.first);

	/* second batch over the rest (non-empty -> false) */
	m = N - k;
	if (m > 8)
		m = 8;
	first = -1;
	for (i = k; i < k + m; i++) {
		if (i == k)
			first = i;
		else
			pool[prev].node.next = &pool[i].node;
		pool[i].on_list = 1;
		prev = i;
	}
	if (m) {
		pool[k + m - 1].node.next = NULL;
		r = xs64();
		if (r & 1)
			fold(llist_add_batch(&pool[first].node,
					     &pool[k + m - 1].node, &head));
		else
			fold(__llist_add_batch(&pool[first].node,
					       &pool[k + m - 1].node, &head));
	} else {
		fold(0xbad4);
	}
	fold_chain_entry_safe(head.first);
	return g_h;
}
