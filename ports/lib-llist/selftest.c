/* Invariant self-test (not part of the shipped port): hand-computed
 * sequences plus a seeded op mix checked against an array-model stack —
 * an llist is exactly a LIFO stack: add pushes front, del_first pops,
 * del_all drains. Exit 0 = all good. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <linux/types.h>
#include <linux/llist.h>

#define N 32
struct tnode { uint64_t val; struct llist_node node; int on_list; };
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
static int nidx(const struct llist_node *p)
{ return p ? (int)(container_of(p, struct tnode, node) - pool) : -1; }

static int fails;
#define CHECK(c, ...) do { if (!(c)) { fails++; \
	printf("FAIL %d: ", __LINE__); printf(__VA_ARGS__); \
	printf("\n"); if (fails > 20) exit(1); } } while (0)

/* model = expected chain, model[0] is head->first */
static int model[N], mlen;
static void check_chain(void)
{
	struct llist_node *p;
	int i = 0;
	llist_for_each(p, &head == NULL ? NULL : head.first) {
		CHECK(i < mlen, "chain longer than model (%d)", i);
		if (i < mlen)
			CHECK(nidx(p) == model[i],
			      "pos %d: have %d want %d", i, nidx(p), model[i]);
		i++;
	}
	CHECK(i == mlen, "chain len %d want %d", i, mlen);
	CHECK(llist_empty(&head) == (mlen == 0), "empty mismatch");
}

int main(void)
{
	struct llist_node *p;
	int i, s;

	/* ---- hand-computed: 3-node add/del_first order ---- */
	init_llist_head(&head);
	memset(pool, 0, sizeof pool);
	CHECK(llist_empty(&head), "not empty after init");
	CHECK(llist_add(&pool[0].node, &head), "add to empty not true");
	pool[0].on_list = 1;
	CHECK(!llist_add(&pool[1].node, &head), "add to nonempty not false");
	pool[1].on_list = 1;
	CHECK(!llist_add(&pool[2].node, &head), "add3 not false");
	pool[2].on_list = 1;
	/* chain must be 2,1,0 */
	{ int want[3] = {2,1,0}, k = 0;
	  llist_for_each(p, head.first) CHECK(nidx(p) == want[k++], "order");
	  CHECK(k == 3, "len %d", k); }
	CHECK(nidx(llist_del_first(&head)) == 2, "del_first != 2");
	pool[2].on_list = 0;
	CHECK(!llist_del_first_this(&head, &pool[0].node),
	      "del_first_this(0) mid should fail");
	CHECK(llist_del_first_this(&head, &pool[1].node), "del_this(1)");
	pool[1].on_list = 0;
	CHECK(!llist_del_first_this(&head, &pool[9].node),
	      "del_first_this(absent) should fail");
	p = llist_del_first_init(&head);
	CHECK(nidx(p) == 0 && !llist_on_list(p), "del_first_init");
	CHECK(llist_del_first(&head) == NULL, "del_first on empty");
	CHECK(llist_empty(&head), "not empty at end");

	/* reverse: 0,1,2 -> del_all 2,1,0 -> reversed 0,1,2 */
	llist_add(&pool[0].node, &head);
	llist_add(&pool[1].node, &head);
	llist_add(&pool[2].node, &head);
	p = llist_del_all(&head);
	CHECK(nidx(p) == 2, "del_all head != 2");
	CHECK(llist_empty(&head), "not empty after del_all");
	p = llist_reverse_order(p);
	{ int want[3] = {0,1,2}, k = 0;
	  llist_for_each(p, p) CHECK(nidx(p) == want[k++], "rev order");
	  CHECK(k == 3, "rev len %d", k); }
	CHECK(llist_reverse_order(NULL) == NULL, "reverse NULL");
	p = &pool[5].node; p->next = NULL;
	CHECK(llist_reverse_order(p) == p, "reverse single");

	/* add_batch: empty->true then nonempty->false, order kept */
	init_llist_head(&head);
	pool[3].node.next = &pool[4].node;
	pool[4].node.next = NULL;
	CHECK(llist_add_batch(&pool[3].node, &pool[4].node, &head),
	      "add_batch to empty");
	{ int want[2] = {3,4}, k = 0;
	  llist_for_each(p, head.first) CHECK(nidx(p) == want[k++], "batch");
	  CHECK(k == 2, "batch len"); }
	pool[5].node.next = NULL;
	CHECK(!llist_add_batch(&pool[5].node, &pool[5].node, &head),
	      "add_batch to nonempty");
	CHECK(nidx(head.first) == 5, "batch head");
	CHECK(llist_del_all(&head) != NULL, "del_all");
	CHECK(llist_del_all(&head) == NULL, "del_all empty");
	CHECK(__llist_del_all(&head) == NULL, "__del_all empty");

	/* ---- seeded model-stack fuzz ---- */
	for (s = 0; s < 40; s++) {
		int nops = 200, j;
		memset(pool, 0, sizeof pool);
		init_llist_head(&head);
		mlen = 0;
		rng_state = 0x9e3779b97f4a7c15ULL + s;
		for (j = 0; j < nops; j++) {
			uint64_t r = xs64();
			switch (r % 6) {
			case 0: case 1: case 2: {	/* push */
				int idx = -1;
				for (i = 0; i < N; i++)
					if (!pool[i].on_list) { idx = i; break; }
				if (idx < 0) break;
				CHECK(llist_add(&pool[idx].node, &head) ==
				      (mlen == 0), "add ret");
				pool[idx].on_list = 1;
				memmove(model + 1, model, mlen * sizeof(int));
				model[0] = idx; mlen++;
				break;
			}
			case 3: {	/* pop */
				p = llist_del_first(&head);
				CHECK(nidx(p) == (mlen ? model[0] : -1),
				      "pop ret");
				if (p) {
					pool[model[0]].on_list = 0;
					memmove(model, model + 1,
						--mlen * sizeof(int));
				}
				break;
			}
			case 4: {	/* drain */
				p = llist_del_all(&head);
				CHECK(nidx(p) == (mlen ? model[0] : -1),
				      "drain ret");
				for (i = 0; i < mlen; i++)
					pool[model[i]].on_list = 0;
				mlen = 0;
				break;
			}
			case 5: {	/* reverse in place */
				int a, b;
				p = __llist_del_all(&head);
				head.first = llist_reverse_order(p);
				for (a = 0, b = mlen - 1; a < b; a++, b--) {
					int t = model[a];
					model[a] = model[b];
					model[b] = t;
				}
				break;
			}
			}
			check_chain();
		}
	}

	if (fails) {
		printf("selftest: %d FAILURES\n", fails);
		return 1;
	}
	printf("selftest: all checks passed (40 fuzz seeds, 3-node order verified)\n");
	return 0;
}
