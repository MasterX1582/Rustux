#pragma once
/* CONFIG_CRYPTO_FIPS is unset in the harness: the self-test blocks in
 * lib/crypto/sha*.c are compiled out and fips_enabled folds to 0, exactly
 * like upstream <linux/fips.h> on a non-FIPS build. */
#define fips_enabled 0
