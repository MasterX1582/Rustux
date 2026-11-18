/* Scalar-field wrappers: expose sort()/sort_r()/list_sort() to the C-ABI
 * bridge by driving them on deterministic seeded inputs and returning
 * scalars (checksums / single elements). The record/diff pipeline verifies
 * the real functions transitively. */

#include <linux/types.h>
#include <linux/sort.h>
#include <linux/list.h>
#include <linux/list_sort.h>
#include <stdint.h>

#define N 64

/* xorshift64: same stream in the C reference and the Rust port. */
static uint64_t rng_state;

static uint64_t xorshift(void)
{
	uint64_t x = rng_state;

	x ^= x << 13;
	x ^= x >> 7;
	x ^= x << 17;
	return rng_state = x;
}

static void seed_rng(uint64_t s)
{
	rng_state = s | 1;
}

static uint64_t checksum(const uint32_t *a, int n)
{
	uint64_t h = 0x9e3779b97f4a7c15ULL;
	int i;

	for (i = 0; i < n; i++)
		h = (h ^ a[i]) * 0x100000001b3ULL;
	return h;
}

static int cmp_u32(const void *a, const void *b)
{
	uint32_t x = *(const uint32_t *)a, y = *(const uint32_t *)b;

	return (x > y) - (x < y);
}

static int cmp_u32_r(const void *a, const void *b, const void *priv)
{
	(void)priv;
	return cmp_u32(a, b);
}

static uint32_t arr32[N];
static uint64_t arr64[N];
static uint8_t arr8[N];

static void fill32(uint64_t seed)
{
	int i;

	seed_rng(seed);
	for (i = 0; i < N; i++)
		arr32[i] = (uint32_t)xorshift();
}

/* sort() on a u32 array; returns an order-sensitive checksum. */
uint64_t sort_u32_sum(uint64_t seed)
{
	fill32(seed);
	sort(arr32, N, sizeof(uint32_t), cmp_u32, NULL);
	return checksum(arr32, N);
}

/* sort() on a u32 array; returns element idx%N after sorting. */
uint32_t sort_u32_elem(uint64_t seed, int64_t idx)
{
	fill32(seed);
	sort(arr32, N, sizeof(uint32_t), cmp_u32, NULL);
	return arr32[idx % N < 0 ? idx % N + N : idx % N];
}

/* sort_r() with a priv-carrying comparator: exercises the cmp_r branch. */
uint64_t sort_r_u32_sum(uint64_t seed)
{
	fill32(seed);
	sort_r(arr32, N, sizeof(uint32_t), cmp_u32_r, NULL, (void *)0x1);
	return checksum(arr32, N);
}

static int cmp_u64(const void *a, const void *b)
{
	uint64_t x = *(const uint64_t *)a, y = *(const uint64_t *)b;

	return (x > y) - (x < y);
}

/* sort() on a u64 array with a real comparator: exercises swap_words_64
 * end to end, checksum of full-width values. */
uint64_t sort_u64_elems(uint64_t seed)
{
	uint64_t h = 0x9e3779b97f4a7c15ULL;
	int i;

	seed_rng(seed);
	for (i = 0; i < N; i++)
		arr64[i] = xorshift();
	sort(arr64, N, sizeof(uint64_t), cmp_u64, NULL);
	for (i = 0; i < N; i++)
		h = (h ^ arr64[i]) * 0x100000001b3ULL;
	return h;
}

static int cmp_u8(const void *a, const void *b)
{
	uint8_t x = *(const uint8_t *)a, y = *(const uint8_t *)b;

	return (x > y) - (x < y);
}

/* sort() on a u8 array with comparator: swap_bytes verified end to end. */
uint64_t sort_u8_elems(uint64_t seed)
{
	uint64_t h = 0x9e3779b97f4a7c15ULL;
	int i;

	seed_rng(seed);
	for (i = 0; i < N; i++)
		arr8[i] = (uint8_t)xorshift();
	sort(arr8, N, sizeof(uint8_t), cmp_u8, NULL);
	for (i = 0; i < N; i++)
		h = (h ^ arr8[i]) * 0x100000001b3ULL;
	return h;
}

/* sort() on only the first `n` elements (n mod N+1): covers num=0/1 edges
 * and partial sorts; checksum over the whole array detects both sorted
 * and untouched regions. */
uint64_t sort_u32_partial(uint64_t seed, int64_t n)
{
	/* Defined-behavior absolute value: 0-n in uint64 avoids the
	 * INT64_MIN signed-overflow UB. */
	size_t cnt = (size_t)(n < 0 ? 0ULL - (uint64_t)n : (uint64_t)n) % (N + 1);

	fill32(seed);
	sort(arr32, cnt, sizeof(uint32_t), cmp_u32, NULL);
	return checksum(arr32, N);
}

/* ---- list_sort ------------------------------------------------------- */

struct node {
	uint32_t key;
	struct list_head lh;
};

static struct node nodes[N];
static struct list_head head = LIST_HEAD_INIT(head);

static int cmp_node(void *priv, const struct list_head *a,
		    const struct list_head *b)
{
	const struct node *na = list_entry(a, struct node, lh);
	const struct node *nb = list_entry(b, struct node, lh);

	(void)priv;
	return (na->key > nb->key) - (na->key < nb->key);
}

static void fill_list(uint64_t seed, unsigned mod)
{
	int i;

	INIT_LIST_HEAD(&head);
	seed_rng(seed);
	for (i = 0; i < N; i++) {
		nodes[i].key = (uint32_t)xorshift();
		if (mod)
			nodes[i].key %= mod;
		list_add_tail(&nodes[i].lh, &head);
	}
}

static uint64_t list_checksum(void)
{
	uint64_t h = 0x9e3779b97f4a7c15ULL;
	struct list_head *p;

	for (p = head.next; p != &head; p = p->next)
		h = (h ^ list_entry(p, struct node, lh)->key) * 0x100000001b3ULL;
	return h;
}

/* list_sort() on N distinct-ish keys; order-sensitive checksum. */
uint64_t listsort_sum(uint64_t seed)
{
	fill_list(seed, 0);
	list_sort(NULL, &head, cmp_node);
	return list_checksum();
}

/* list_sort() with key%8 collisions: only a STABLE sort yields the same
 * checksum — verifies stability transitively. */
uint64_t listsort_ties_sum(uint64_t seed)
{
	fill_list(seed, 8);
	list_sort(NULL, &head, cmp_node);
	return list_checksum();
}

/* list_sort() on 0..7 elements: covers the empty/singleton early return
 * and small-merge corner cases the fixed-N=64 wrappers never reach. */
uint64_t listsort_small(uint64_t seed, int64_t n)
{
	size_t cnt = (size_t)(n < 0 ? 0ULL - (uint64_t)n : (uint64_t)n) % 8;
	uint64_t h = 0x9e3779b97f4a7c15ULL;
	struct list_head *p;
	size_t i;

	INIT_LIST_HEAD(&head);
	seed_rng(seed);
	for (i = 0; i < cnt; i++) {
		nodes[i].key = (uint32_t)xorshift();
		list_add_tail(&nodes[i].lh, &head);
	}
	list_sort(NULL, &head, cmp_node);
	for (p = head.next; p != &head; p = p->next)
		h = (h ^ list_entry(p, struct node, lh)->key) * 0x100000001b3ULL;
	return h;
}

/* k-th key after sorting; direct element-level check. */
uint32_t listsort_elem(uint64_t seed, int64_t idx)
{
	struct list_head *p;
	int64_t i = idx % N < 0 ? idx % N + N : idx % N;

	fill_list(seed, 0);
	list_sort(NULL, &head, cmp_node);
	for (p = head.next; p != &head && i > 0; p = p->next)
		i--;
	return list_entry(p, struct node, lh)->key;
}
