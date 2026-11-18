#pragma once
/* Deterministic seeded RNG: bucket_table_alloc() folds a random seed
 * into each table's hashes, so the harness drives it from a seeded
 * stream implemented identically on both sides. */
#include <linux/types.h>

u32 kshim_random_u32(void);

#define get_random_u32()	kshim_random_u32()
