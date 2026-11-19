/* Invariant self-test (not part of the shipped port): drives the three
 * wrapper entry points across a fixed seed grid and prints the
 * checksums.  Exercises every op family including the failure-injection
 * masks; a hang, abort or BUG means the harness is broken. */
#include <stdio.h>
#include <stdint.h>

uint64_t wrap_idr_seq(uint64_t seed, int64_t nops_i, int64_t seed2);
uint64_t wrap_ida_seq(uint64_t seed, int64_t nops_i, int64_t seed2);
uint64_t wrap_radix_seq(uint64_t seed, int64_t nops_i, int64_t seed2);

int main(void)
{
	static const uint64_t seeds[] = { 1, 3, 42, 255, 1000, 65535,
		0xdeadbeefULL, 0x123456789ULL };
	static const int64_t s2[] = { 0, 1, 2, 3, 6, 11, 21, 255, -7 };
	uint64_t acc = 0;
	unsigned i, j;
	setbuf(stdout, NULL);
	for (i = 0; i < sizeof seeds / sizeof seeds[0]; i++) {
		for (j = 0; j < sizeof s2 / sizeof s2[0]; j++) {
			uint64_t h1 = wrap_idr_seq(seeds[i], 133, s2[j]);
			uint64_t h2 = wrap_ida_seq(seeds[i], 161, s2[j]);
			uint64_t h3 = wrap_radix_seq(seeds[i], 163, s2[j]);
			acc ^= h1; acc = acc * 0x100000001b3ULL;
			acc ^= h2; acc = acc * 0x100000001b3ULL;
			acc ^= h3; acc = acc * 0x100000001b3ULL;
			printf("seed=%-10llx s2=%-4lld idr=%016llx ida=%016llx rt=%016llx\n",
			       (unsigned long long)seeds[i], (long long)s2[j],
			       (unsigned long long)h1, (unsigned long long)h2,
			       (unsigned long long)h3);
		}
	}
	printf("selftest aggregate %016llx\n", (unsigned long long)acc);
	return 0;
}
