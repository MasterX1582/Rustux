/* Deterministic selftest for the byte-identical lib/crypto/siphash.c.
 *
 * Reuses the upstream test vectors from lib/crypto/tests/siphash_kunit.c
 * (lib/test_siphash.c does not exist in this tree) and exercises:
 *   - siphash() over the ramp input buf[i]=i for every length 0..64,
 *     aligned and unaligned (covers both __siphash backends + every
 *     tail-path case of the switch)
 *   - hsiphash() likewise (SipHash1-3 on LP64)
 *   - the siphash_{1..4}u64, siphash_{1..4}u32, hsiphash_{1..4}u32 scalars
 *
 * Prints one hex line per case plus a final "SELFTEST ..." aggregate so
 * output can be diffed across builds/compilers.
 */
#include <stdio.h>
#include <stdint.h>
#include "linux/siphash.h"

/* Test vectors taken from reference source available at:
 *     https://github.com/veorq/SipHash
 * (as embedded in lib/crypto/tests/siphash_kunit.c, BITS_PER_LONG == 64) */
static const siphash_key_t test_key_siphash =
	{{ 0x0706050403020100ULL, 0x0f0e0d0c0b0a0908ULL }};

static const u64 test_vectors_siphash[64] = {
	0x726fdb47dd0e0e31ULL, 0x74f839c593dc67fdULL, 0x0d6c8009d9a94f5aULL,
	0x85676696d7fb7e2dULL, 0xcf2794e0277187b7ULL, 0x18765564cd99a68dULL,
	0xcbc9466e58fee3ceULL, 0xab0200f58b01d137ULL, 0x93f5f5799a932462ULL,
	0x9e0082df0ba9e4b0ULL, 0x7a5dbbc594ddb9f3ULL, 0xf4b32f46226bada7ULL,
	0x751e8fbc860ee5fbULL, 0x14ea5627c0843d90ULL, 0xf723ca908e7af2eeULL,
	0xa129ca6149be45e5ULL, 0x3f2acc7f57c29bdbULL, 0x699ae9f52cbe4794ULL,
	0x4bc1b3f0968dd39cULL, 0xbb6dc91da77961bdULL, 0xbed65cf21aa2ee98ULL,
	0xd0f2cbb02e3b67c7ULL, 0x93536795e3a33e88ULL, 0xa80c038ccd5ccec8ULL,
	0xb8ad50c6f649af94ULL, 0xbce192de8a85b8eaULL, 0x17d835b85bbb15f3ULL,
	0x2f2e6163076bcfadULL, 0xde4daaaca71dc9a5ULL, 0xa6a2506687956571ULL,
	0xad87a3535c49ef28ULL, 0x32d892fad841c342ULL, 0x7127512f72f27cceULL,
	0xa7f32346f95978e3ULL, 0x12e0b01abb051238ULL, 0x15e034d40fa197aeULL,
	0x314dffbe0815a3b4ULL, 0x027990f029623981ULL, 0xcadcd4e59ef40c4dULL,
	0x9abfd8766a33735cULL, 0x0e3ea96b5304a7d0ULL, 0xad0c42d6fc585992ULL,
	0x187306c89bc215a9ULL, 0xd4a60abcf3792b95ULL, 0xf935451de4f21df2ULL,
	0xa9538f0419755787ULL, 0xdb9acddff56ca510ULL, 0xd06c98cd5c0975ebULL,
	0xe612a3cb9ecba951ULL, 0xc766e62cfcadaf96ULL, 0xee64435a9752fe72ULL,
	0xa192d576b245165aULL, 0x0a8787bf8ecb74b2ULL, 0x81b3e73d20b49b6fULL,
	0x7fa8220ba3b2eceaULL, 0x245731c13ca42499ULL, 0xb78dbfaf3a8d83bdULL,
	0xea1ad565322a1a0bULL, 0x60e61c23a3795013ULL, 0x6606d7e446282b93ULL,
	0x6ca4ecb15c5f91e1ULL, 0x9f626da15c9625f3ULL, 0xe51b38608ef25f57ULL,
	0x958a324ceb064572ULL
};

static const hsiphash_key_t test_key_hsiphash =
	{{ 0x0706050403020100ULL, 0x0f0e0d0c0b0a0908ULL }};

static const u32 test_vectors_hsiphash[64] = {
	0x050fc4dcU, 0x7d57ca93U, 0x4dc7d44dU,
	0xe7ddf7fbU, 0x88d38328U, 0x49533b67U,
	0xc59f22a7U, 0x9bb11140U, 0x8d299a8eU,
	0x6c063de4U, 0x92ff097fU, 0xf94dc352U,
	0x57b4d9a2U, 0x1229ffa7U, 0xc0f95d34U,
	0x2a519956U, 0x7d908b66U, 0x63dbd80cU,
	0xb473e63eU, 0x8d297d1cU, 0xa6cce040U,
	0x2b45f844U, 0xa320872eU, 0xdae6c123U,
	0x67349c8cU, 0x705b0979U, 0xca9913a5U,
	0x4ade3b35U, 0xef6cd00dU, 0x4ab1e1f4U,
	0x43c5e663U, 0x8c21d1bcU, 0x16a7b60dU,
	0x7a8ff9bfU, 0x1f2a753eU, 0xbf186b91U,
	0xada26206U, 0xa3c33057U, 0xae3a36a1U,
	0x7b108392U, 0x99e41531U, 0x3f1ad944U,
	0xc8138825U, 0xc28949a6U, 0xfaf8876bU,
	0x9f042196U, 0x68b1d623U, 0x8b5114fdU,
	0xdf074c46U, 0x12cc86b3U, 0x0a52098fU,
	0x9d292f9aU, 0xa2f41f12U, 0x43a71ed0U,
	0x73f0bce6U, 0x70a7e980U, 0x243c6d75U,
	0xfdb71513U, 0xa67d8a08U, 0xb7e8f148U,
	0xf7a644eeU, 0x0f1837f2U, 0x4b6694e0U,
	0xb7bbb3a8U
};

static int fails;
static unsigned long checks;
static u64 acc;

static void note64(const char *name, unsigned i, u64 got, u64 want, int chk)
{
	acc ^= got + 0x9e3779b97f4a7c15ULL + ((u64)checks << 6);
	checks++;
	if (chk && got != want) {
		printf("%s[%u] = 0x%016llx  WANT 0x%016llx  FAIL\n",
		       name, i, (unsigned long long)got,
		       (unsigned long long)want);
		fails++;
	} else {
		printf("%s[%u] = 0x%016llx\n", name, i,
		       (unsigned long long)got);
	}
}

static void note32(const char *name, unsigned i, u32 got, u32 want, int chk)
{
	acc ^= (u64)got + 0x9e3779b97f4a7c15ULL + ((u64)checks << 6);
	checks++;
	if (chk && got != want) {
		printf("%s[%u] = 0x%08x  WANT 0x%08x  FAIL\n",
		       name, i, got, want);
		fails++;
	} else {
		printf("%s[%u] = 0x%08x\n", name, i, got);
	}
}

int main(void)
{
	u8 in[72] __aligned(SIPHASH_ALIGNMENT);
	u8 in_unaligned[80] __aligned(SIPHASH_ALIGNMENT);
	u8 i;

	for (i = 0; i <= 64; ++i) {
		u64 a, u;
		u32 ha, hu;
		int chk = i < 64;
		if (i < 64) {
			in[i] = i;
			in_unaligned[i + 1] = i;
		}
		a = siphash(in, i, &test_key_siphash);
		u = siphash(in_unaligned + 1, i, &test_key_siphash);
		ha = hsiphash(in, i, &test_key_hsiphash);
		hu = hsiphash(in_unaligned + 1, i, &test_key_hsiphash);
		note64("siphash_aligned", i, a, test_vectors_siphash[i % 64],
		       chk);
		note64("siphash_unaligned", i, u,
		       test_vectors_siphash[i % 64], chk);
		note32("hsiphash_aligned", i, ha,
		       test_vectors_hsiphash[i % 64], chk);
		note32("hsiphash_unaligned", i, hu,
		       test_vectors_hsiphash[i % 64], chk);
	}

	note64("siphash_1u64", 0,
	       siphash_1u64(0x0706050403020100ULL, &test_key_siphash),
	       test_vectors_siphash[8], 1);
	note64("siphash_2u64", 0,
	       siphash_2u64(0x0706050403020100ULL, 0x0f0e0d0c0b0a0908ULL,
			    &test_key_siphash),
	       test_vectors_siphash[16], 1);
	note64("siphash_3u64", 0,
	       siphash_3u64(0x0706050403020100ULL, 0x0f0e0d0c0b0a0908ULL,
			    0x1716151413121110ULL, &test_key_siphash),
	       test_vectors_siphash[24], 1);
	note64("siphash_4u64", 0,
	       siphash_4u64(0x0706050403020100ULL, 0x0f0e0d0c0b0a0908ULL,
			    0x1716151413121110ULL, 0x1f1e1d1c1b1a1918ULL,
			    &test_key_siphash),
	       test_vectors_siphash[32], 1);
	note64("siphash_1u32", 0,
	       siphash_1u32(0x03020100U, &test_key_siphash),
	       test_vectors_siphash[4], 1);
	note64("siphash_2u32", 0,
	       siphash_2u32(0x03020100U, 0x07060504U, &test_key_siphash),
	       test_vectors_siphash[8], 1);
	note64("siphash_3u32", 0,
	       siphash_3u32(0x03020100U, 0x07060504U, 0x0b0a0908U,
			    &test_key_siphash),
	       test_vectors_siphash[12], 1);
	note64("siphash_4u32", 0,
	       siphash_4u32(0x03020100U, 0x07060504U, 0x0b0a0908U,
			    0x0f0e0d0cU, &test_key_siphash),
	       test_vectors_siphash[16], 1);
	note32("hsiphash_1u32", 0,
	       hsiphash_1u32(0x03020100U, &test_key_hsiphash),
	       test_vectors_hsiphash[4], 1);
	note32("hsiphash_2u32", 0,
	       hsiphash_2u32(0x03020100U, 0x07060504U, &test_key_hsiphash),
	       test_vectors_hsiphash[8], 1);
	note32("hsiphash_3u32", 0,
	       hsiphash_3u32(0x03020100U, 0x07060504U, 0x0b0a0908U,
			     &test_key_hsiphash),
	       test_vectors_hsiphash[12], 1);
	note32("hsiphash_4u32", 0,
	       hsiphash_4u32(0x03020100U, 0x07060504U, 0x0b0a0908U,
			     0x0f0e0d0cU, &test_key_hsiphash),
	       test_vectors_hsiphash[16], 1);

	printf("SELFTEST %s checks=%lu fails=%d acc=0x%016llx\n",
	       fails ? "FAIL" : "PASS", checks, fails,
	       (unsigned long long)acc);
	return fails != 0;
}
