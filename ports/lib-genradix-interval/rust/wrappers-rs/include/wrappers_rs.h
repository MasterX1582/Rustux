#pragma once
/* C ABI of the wrappers-rs port. Mirrors the prototypes in
 * wrappers.c that rust-anything.json exports. */
#include <stdint.h>

uint64_t wrap_genradix_run(uint64_t seed, int64_t nops_i, int64_t variant_i,
			   int64_t budget_i);
uint64_t wrap_genradix_seq(uint64_t seed, int64_t n_i, int64_t variant_i);
uint64_t wrap_interval_run(uint64_t seed, int64_t nops_i, int64_t variant_i);
uint64_t wrap_interval_seq(uint64_t seed, int64_t n_i, int64_t variant_i);
