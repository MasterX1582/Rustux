/* Independent sanity checks for the lib-sha port unit: canonical
 * published digest/HMAC vectors, not the recorded corpus.  Prints FAIL
 * lines and returns the failure count. */
#include <stdint.h>
#include <stddef.h>
#include <string.h>
#include <stdio.h>

#include <crypto/sha1.h>
#include <crypto/sha2.h>
#include <crypto/hmac.h>

static int fails;

static int hexval(char c)
{
	if (c >= '0' && c <= '9') return c - '0';
	if (c >= 'a' && c <= 'f') return c - 'a' + 10;
	if (c >= 'A' && c <= 'F') return c - 'A' + 10;
	return -1;
}

/* Compare a digest against its lowercase hex form. */
static void ckhex(const char *name, const u8 *d, size_t n, const char *hex)
{
	size_t i;
	for (i = 0; i < n; i++) {
		int hi = hexval(hex[2 * i]), lo = hexval(hex[2 * i + 1]);
		if (hi < 0 || lo < 0 || d[i] != (u8)((hi << 4) | lo)) {
			printf("FAIL %s: byte %zu got %02x want %s\n",
			       name, i, d[i], hex);
			fails++;
			return;
		}
	}
	if (hex[2 * n]) {
		printf("FAIL %s: expected digest longer than %zu\n", name, n);
		fails++;
	}
}

int main(void)
{
	u8 d[SHA512_DIGEST_SIZE];
	u8 d2[SHA256_DIGEST_SIZE];
	static const char msg[] = "The quick brown fox jumps over the lazy dog";
	static u8 mil[1000000];
	struct sha256_ctx sctx;
	struct sha1_ctx c1;
	struct sha512_ctx c5;
	struct hmac_sha256_key hkey;
	struct hmac_sha256_ctx hctx;
	static u8 bigkey[131];
	size_t i;

	memset(mil, 'a', sizeof mil);

	/* ---- one-shot canonical vectors ---- */
	sha1((const u8 *)"abc", 3, d);
	ckhex("sha1(abc)", d, SHA1_DIGEST_SIZE,
	      "a9993e364706816aba3e25717850c26c9cd0d89d");
	sha1((const u8 *)"", 0, d);
	ckhex("sha1()", d, SHA1_DIGEST_SIZE,
	      "da39a3ee5e6b4b0d3255bfef95601890afd80709");
	sha1(mil, sizeof mil, d);
	ckhex("sha1(a*1e6)", d, SHA1_DIGEST_SIZE,
	      "34aa973cd4c4daa4f61eeb2bdbad27316534016f");

	sha224((const u8 *)"abc", 3, d);
	ckhex("sha224(abc)", d, SHA224_DIGEST_SIZE,
	      "23097d223405d8228642a477bda255b32aadbce4bda0b3f7e36c9da7");
	sha224((const u8 *)"", 0, d);
	ckhex("sha224()", d, SHA224_DIGEST_SIZE,
	      "d14a028c2a3a2bc9476102bb288234c415a2b01f828ea62ac5b3e42f");
	sha224(mil, sizeof mil, d);
	ckhex("sha224(a*1e6)", d, SHA224_DIGEST_SIZE,
	      "20794655980c91d8bbb4c1ea97618a4bf03f42581948b2ee4ee7ad67");

	sha256((const u8 *)"abc", 3, d);
	ckhex("sha256(abc)", d, SHA256_DIGEST_SIZE,
	      "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
	sha256((const u8 *)"", 0, d);
	ckhex("sha256()", d, SHA256_DIGEST_SIZE,
	      "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
	sha256(mil, sizeof mil, d);
	ckhex("sha256(a*1e6)", d, SHA256_DIGEST_SIZE,
	      "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0");

	sha384((const u8 *)"abc", 3, d);
	ckhex("sha384(abc)", d, SHA384_DIGEST_SIZE,
	      "cb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed"
	      "8086072ba1e7cc2358baeca134c825a7");
	sha384((const u8 *)"", 0, d);
	ckhex("sha384()", d, SHA384_DIGEST_SIZE,
	      "38b060a751ac96384cd9327eb1b1e36a21fdb71114be07434c0cc7bf63f6e1da"
	      "274edebfe76f65fbd51ad2f14898b95b");
	sha384(mil, sizeof mil, d);
	ckhex("sha384(a*1e6)", d, SHA384_DIGEST_SIZE,
	      "9d0e1809716474cb086e834e310a4a1ced149e9c00f248527972cec5704c2a"
	      "5b07b8b3dc38ecc4ebae97ddd87f3d8985");

	sha512((const u8 *)"abc", 3, d);
	ckhex("sha512(abc)", d, SHA512_DIGEST_SIZE,
	      "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a"
	      "2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f");
	sha512((const u8 *)"", 0, d);
	ckhex("sha512()", d, SHA512_DIGEST_SIZE,
	      "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce"
	      "47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e");
	sha512(mil, sizeof mil, d);
	ckhex("sha512(a*1e6)", d, SHA512_DIGEST_SIZE,
	      "e718483d0ce769644e2e42c7bc15b4638e1f98b13b2044285632a803afa973eb"
	      "de0ff244877ea60a4cb0432ce577c31beb009c5c2c49aa2e4eadb217ad8cc09b");

	/* ---- streaming equals one-shot, incl. 1-byte chunks ---- */
	sha256_init(&sctx);
	for (i = 0; i < 64; i++)
		sha256_update(&sctx, mil + i, 1);
	sha256_update(&sctx, mil + 64, 936);
	sha256_final(&sctx, d);
	sha256(mil, 1000, d2);
	if (memcmp(d, d2, SHA256_DIGEST_SIZE)) {
		printf("FAIL sha256 streaming != one-shot\n");
		fails++;
	}

	sha1_init(&c1);
	sha1_update(&c1, mil, 55);
	sha1_update(&c1, mil + 55, 1);	/* land exactly on the pad edge */
	sha1_update(&c1, mil + 56, 8);	/* cross the length-field seam */
	sha1_update(&c1, mil + 64, 936);
	sha1_final(&c1, d);
	sha1(mil, 1000, d2);
	if (memcmp(d, d2, SHA1_DIGEST_SIZE)) {
		printf("FAIL sha1 streaming != one-shot\n");
		fails++;
	}

	sha512_init(&c5);
	sha512_update(&c5, mil, 111);
	sha512_update(&c5, mil + 111, 1);	/* 112: the SHA-512 pad edge */
	sha512_update(&c5, mil + 112, 16);
	sha512_update(&c5, mil + 128, 872);
	sha512_final(&c5, d);
	sha512(mil, 1000, d2);
	if (memcmp(d, d2, SHA512_DIGEST_SIZE)) {
		printf("FAIL sha512 streaming != one-shot\n");
		fails++;
	}

	/* ---- sha256_finup_2x: NULL ctx and pre-fed ctx ---- */
	sha256_finup_2x(NULL, (const u8 *)"abc", (const u8 *)"def", 3, d, d2);
	{
		u8 e1[SHA256_DIGEST_SIZE], e2[SHA256_DIGEST_SIZE];
		sha256((const u8 *)"abc", 3, e1);
		sha256((const u8 *)"def", 3, e2);
		if (memcmp(d, e1, 32) || memcmp(d2, e2, 32)) {
			printf("FAIL sha256_finup_2x NULL ctx\n");
			fails++;
		}
	}
	sha256_init(&sctx);
	sha256_update(&sctx, (const u8 *)"pre", 3);
	sha256_finup_2x(&sctx, (const u8 *)"abc", (const u8 *)"def", 3, d, d2);
	{
		u8 e1[SHA256_DIGEST_SIZE], e2[SHA256_DIGEST_SIZE];
		sha256((const u8 *)"preabc", 6, e1);
		sha256((const u8 *)"predef", 6, e2);
		if (memcmp(d, e1, 32) || memcmp(d2, e2, 32)) {
			printf("FAIL sha256_finup_2x prefed ctx\n");
			fails++;
		}
	}
	if (sha256_finup_2x_is_optimized()) {
		printf("FAIL sha256_finup_2x_is_optimized true on generic path\n");
		fails++;
	}

	/* ---- HMAC canonical vectors ---- */
	{
		static const u8 key3[] = "key";
		hmac_sha1_usingrawkey(key3, 3, (const u8 *)msg, strlen(msg), d);
		ckhex("hmac-sha1(key)", d, SHA1_DIGEST_SIZE,
		      "de7c9b85b8b78aa6bc8a7a36f70a90701c9db4d9");
		hmac_sha224_usingrawkey(key3, 3, (const u8 *)msg, strlen(msg), d);
		ckhex("hmac-sha224(key)", d, SHA224_DIGEST_SIZE,
		      "88ff8b54675d39b8f72322e65ff945c52d96379988ada25639747e69");
		hmac_sha256_usingrawkey(key3, 3, (const u8 *)msg, strlen(msg), d);
		ckhex("hmac-sha256(key)", d, SHA256_DIGEST_SIZE,
		      "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8");
		hmac_sha384_usingrawkey(key3, 3, (const u8 *)msg, strlen(msg), d);
		ckhex("hmac-sha384(key)", d, SHA384_DIGEST_SIZE,
		      "d7f4727e2c0b39ae0f1e40cc96f60242d5b7801841cea6fc592c5d3e1ae507"
		      "00582a96cf35e1e554995fe4e03381c237");
		hmac_sha512_usingrawkey(key3, 3, (const u8 *)msg, strlen(msg), d);
		ckhex("hmac-sha512(key)", d, SHA512_DIGEST_SIZE,
		      "b42af09057bac1e2d41708e48a902e09b5ff7f12ab428a4fe86653c73dd248"
		      "fb82f948a549f7b791a5b41915ee4d1ec3935357e4e2317250d0372afa2ebeeb3a");
	}

	/* RFC 2202 test case 1: HMAC-SHA1(key=0x0b*20, "Hi There") */
	{
		u8 k[20];
		memset(k, 0x0b, sizeof k);
		hmac_sha1_usingrawkey(k, sizeof k, (const u8 *)"Hi There", 8, d);
		ckhex("hmac-sha1 rfc2202.1", d, SHA1_DIGEST_SIZE,
		      "b617318655057264e28bc0b6fb378c8ef146be00");
	}

	/* Key longer than block size -> key gets hashed first. */
	memset(bigkey, 0xaa, sizeof bigkey);
	hmac_sha256_usingrawkey(bigkey, 131,
		(const u8 *)"Test Using Larger Than Block-Size Key - Hash Key First",
		54, d);
	ckhex("hmac-sha256 k131", d, SHA256_DIGEST_SIZE,
	      "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54");
	hmac_sha224_usingrawkey(bigkey, 131,
		(const u8 *)"Test Using Larger Than Block-Size Key - Hash Key First",
		54, d);
	ckhex("hmac-sha224 k131", d, SHA224_DIGEST_SIZE,
	      "95e9a0db962095adaebe9b2d6f0dbce2d499f112f2d2b7273fa6870e");
	hmac_sha384_usingrawkey(bigkey, 131,
		(const u8 *)"Test Using Larger Than Block-Size Key - Hash Key First",
		54, d);
	ckhex("hmac-sha384 k131", d, SHA384_DIGEST_SIZE,
	      "4ece084485813e9088d2c63a041bc5b44f9ef1012a2b588f3cd11f05033ac4c6"
	      "0c2ef6ab4030fe8296248df163f44952");
	hmac_sha512_usingrawkey(bigkey, 129,
		(const u8 *)"Test Using Larger Than Block-Size Key - Hash Key First",
		54, d);
	ckhex("hmac-sha512 k129", d, SHA512_DIGEST_SIZE,
	      "43a4b1f8caa79d9bd189d279990481e79a3096d8ec2aec3901f2bf8a894c89"
	      "462c21a3e902207de792fce9bfb2d52beb04289022ebaacfbadc99bae86d3c7c6d");
	hmac_sha1_usingrawkey(bigkey, 80,
		(const u8 *)"Test Using Larger Than Block-Size Key - Hash Key First",
		54, d);
	ckhex("hmac-sha1 k80", d, SHA1_DIGEST_SIZE,
	      "aa4ae5e15272d00e95705637ce8a3b55ed402112");

	/* Prepared-key path must agree with the raw-key path. */
	hmac_sha256_preparekey(&hkey, (const u8 *)"key", 3);
	hmac_sha256(&hkey, (const u8 *)msg, strlen(msg), d);
	ckhex("hmac-sha256 prepared", d, SHA256_DIGEST_SIZE,
	      "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8");
	hmac_sha256_init(&hctx, &hkey);
	hmac_sha256_update(&hctx, (const u8 *)msg, 20);
	hmac_sha256_update(&hctx, (const u8 *)msg + 20, strlen(msg) - 20);
	hmac_sha256_final(&hctx, d);
	ckhex("hmac-sha256 prepared stream", d, SHA256_DIGEST_SIZE,
	      "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8");

	if (!fails)
		printf("selftest: all checks passed\n");
	return fails;
}
