#pragma once
/* linux/random.h shim — DETERMINISTIC replacement for the harness.
 *
 * Upstream get_random_u32_below() draws from the kernel CSPRNG, which is
 * non-deterministic and therefore unverifiable. The harness instead keeps
 * a splitmix64 state seeded by the wrapper (wrappers.c sets it from a
 * scalar argument before calling find_random_bit()); the Rust port
 * replicates the same generator, so both sides observe identical draws
 * and the find_nth_bit path inside find_random_bit() is verified.
 * The value is returned modulo the bound (like any correct draw < bound,
 * this only changes which bit is picked, never the algorithm). */
#include <linux/types.h>

extern u64 __harness_rand_state;

static inline void __harness_seed_random(u64 seed)
{
	__harness_rand_state = seed | 1;
}

static inline u32 get_random_u32_below(u32 bound)
{
	u64 z;

	__harness_rand_state += 0x9e3779b97f4a7c15ULL;
	z = __harness_rand_state;
	z = (z ^ (z >> 30)) * 0xbf58476d1ce4e5b9ULL;
	z = (z ^ (z >> 27)) * 0x94d049bb133111ebULL;
	z = z ^ (z >> 31);
	return (u32)(z % bound);
}

static inline u32 get_random_u32(void)
{
	return get_random_u32_below(0xffffffffU);
}
