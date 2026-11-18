    use super::*;

    fn hex(d: &[u8]) -> [u8; 128] {
        let mut s = [0u8; 128];
        const H: &[u8; 16] = b"0123456789abcdef";
        for (i, &b) in d.iter().enumerate() {
            s[2 * i] = H[(b >> 4) as usize];
            s[2 * i + 1] = H[(b & 15) as usize];
        }
        s
    }

    fn hex_eq(d: &[u8], want: &[u8]) -> bool {
        let h = hex(d);
        h[..want.len()] == *want
    }

    #[test]
    fn sha1_vectors() {
        let mut out = [0u8; 20];
        sha1(b"abc", 3, &mut out);
        assert!(hex_eq(&out, b"a9993e364706816aba3e25717850c26c9cd0d89d"));
        sha1(b"", 0, &mut out);
        assert_eq!(out, SHA1_ZERO_MESSAGE_HASH);
        sha1(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq", 56, &mut out);
        assert!(hex_eq(&out, b"84983e441c3bd26ebaae4aa1f95129e5e54670f1"));
    }

    #[test]
    fn sha224_sha256_vectors() {
        let mut out = [0u8; 32];
        sha256(b"abc", 3, &mut out);
        assert!(hex_eq(&out, b"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"));
        sha256(b"", 0, &mut out);
        assert_eq!(out, SHA256_ZERO_MESSAGE_HASH);
        sha256(
            b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq",
            56,
            &mut out,
        );
        assert!(hex_eq(&out, b"248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"));
        let mut out28 = [0u8; 28];
        sha224(b"abc", 3, &mut out28);
        assert!(hex_eq(&out28, b"23097d223405d8228642a477bda255b32aadbce4bda0b3f7e36c9da7"));
        sha224(b"", 0, &mut out28);
        assert_eq!(out28, SHA224_ZERO_MESSAGE_HASH);
    }

    #[test]
    fn sha384_sha512_vectors() {
        let mut out = [0u8; 64];
        sha512(b"abc", 3, &mut out);
        assert!(hex_eq(
            &out,
            b"ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"
        ));
        sha512(b"", 0, &mut out);
        assert_eq!(out, SHA512_ZERO_MESSAGE_HASH);
        let mut out48 = [0u8; 48];
        sha384(b"abc", 3, &mut out48);
        assert!(hex_eq(
            &out48,
            b"cb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed8086072ba1e7cc2358baeca134c825a7"
        ));
        sha384(b"", 0, &mut out48);
        assert_eq!(out48, SHA384_ZERO_MESSAGE_HASH);
    }

    #[test]
    fn streaming_matches_one_shot() {
        // Every chunk size must land the same digest, incl. partial blocks.
        let msg = {
            let mut m = [0u8; 200];
            let mut s = 0x9e3779b97f4a7c15u64;
            for b in m.iter_mut() {
                s ^= s << 13;
                s ^= s >> 7;
                s ^= s << 17;
                *b = s as u8;
            }
            m
        };
        for &chunk in &[1usize, 3, 55, 56, 63, 64, 65, 127, 128, 199, 200] {
            let mut ctx = Sha256Ctx::default();
            sha256_init(&mut ctx);
            let mut i = 0;
            while i < msg.len() {
                let n = chunk.min(msg.len() - i);
                sha256_update(&mut ctx, &msg[i..i + n], n);
                i += n;
            }
            let mut d = [0u8; 32];
            sha256_final(&mut ctx, &mut d);
            let mut want = [0u8; 32];
            sha256(&msg, msg.len(), &mut want);
            assert_eq!(d, want, "chunk={}", chunk);
        }
    }

    #[test]
    fn hmac_sha256_vector() {
        // RFC 4231 test case 2
        let mut key = HmacSha256Key::default();
        hmac_sha256_preparekey(&mut key, b"Jefe", 4);
        let mut out = [0u8; 32];
        hmac_sha256(
            &key,
            b"what do ya want for nothing?",
            28,
            &mut out,
        );
        assert!(hex_eq(&out, b"5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"));
    }

    #[test]
    fn hmac_sha256_long_key() {
        // RFC 4231 test case 6: 131-byte key (> blocksize, gets hashed)
        let k = [0xaau8; 131];
        let mut key = HmacSha256Key::default();
        hmac_sha256_preparekey(&mut key, &k, k.len());
        let mut out = [0u8; 32];
        hmac_sha256(
            &key,
            b"Test Using Larger Than Block-Size Key - Hash Key First",
            54,
            &mut out,
        );
        assert!(hex_eq(&out, b"60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"));
    }

    #[test]
    fn hmac_rawkey_matches_prepared() {
        let k = [0x0bu8; 20];
        let data = b"Hi There";
        let mut d1 = [0u8; 20];
        let mut d2 = [0u8; 20];
        let mut ctx = HmacSha1Ctx::default();
        hmac_sha1_init_usingrawkey(&mut ctx, &k, k.len());
        hmac_sha1_update(&mut ctx, data, data.len());
        hmac_sha1_final(&mut ctx, &mut d1);
        hmac_sha1_usingrawkey(&k, k.len(), data, data.len(), &mut d2);
        assert_eq!(d1, d2);
        // RFC 2202 case 1
        assert!(hex_eq(&d1, b"b617318655057264e28bc0b6fb378c8ef146be00"));
    }

    #[test]
    fn hmac_sha512_vector() {
        // RFC 4231 test case 2 (SHA-512)
        let mut key = HmacSha512Key::default();
        hmac_sha512_preparekey(&mut key, b"Jefe", 4);
        let mut out = [0u8; 64];
        hmac_sha512(&key, b"what do ya want for nothing?", 28, &mut out);
        assert!(hex_eq(
            &out,
            b"164b7a7bfcf819e2e395fbe73b56e0a387bd64222e831fd610270cd7ea2505549758bf75c05a994a6d034f65f8f0e6fdcaeab1a34d4a6b4b636e070a38bce737"
        ));
    }

    #[test]
    fn finup_2x_matches_pair() {
        let d1 = b"first";
        let d2 = b"second";
        let mut ctx = Sha256Ctx::default();
        sha256_init(&mut ctx);
        let mut o1 = [0u8; 32];
        let mut o2 = [0u8; 32];
        sha256_finup_2x(Some(&ctx), d1, d2, 5, &mut o1, &mut o2);
        let mut w1 = [0u8; 32];
        let mut w2 = [0u8; 32];
        sha256(d1, 5, &mut w1);
        sha256(d2, 5, &mut w2);
        assert_eq!(o1, w1);
        assert_eq!(o2, w2);
        assert!(!sha256_finup_2x_is_optimized());
        // None ctx = fresh INITIAL_SHA256_CTX (C's NULL path).
        sha256_finup_2x(None, d1, d2, 5, &mut o1, &mut o2);
        assert_eq!(o1, w1);
        assert_eq!(o2, w2);
    }

    #[test]
    fn zero_message_hashes_are_empty_digests() {
        let mut d20 = [0u8; 20];
        sha1(b"x", 0, &mut d20); // len 0: content ignored
        assert_eq!(d20, SHA1_ZERO_MESSAGE_HASH);
        let mut d32 = [0u8; 32];
        sha256(b"x", 0, &mut d32);
        assert_eq!(d32, SHA256_ZERO_MESSAGE_HASH);
        let mut d48 = [0u8; 48];
        sha384(b"x", 0, &mut d48);
        assert_eq!(d48, SHA384_ZERO_MESSAGE_HASH);
        let mut d64 = [0u8; 64];
        sha512(b"x", 0, &mut d64);
        assert_eq!(d64, SHA512_ZERO_MESSAGE_HASH);
        let mut d28 = [0u8; 28];
        sha224(b"x", 0, &mut d28);
        assert_eq!(d28, SHA224_ZERO_MESSAGE_HASH);
    }

    #[test]
    fn million_a() {
        // The classic long-message vector through the streaming API.
        let mut ctx = Sha256Ctx::default();
        sha256_init(&mut ctx);
        let chunk = [b'a'; 64];
        for _ in 0..(1_000_000 / 64) {
            sha256_update(&mut ctx, &chunk, 64);
        }
        sha256_update(&mut ctx, &chunk[..1_000_000 % 64], 1_000_000 % 64);
        let mut d = [0u8; 32];
        sha256_final(&mut ctx, &mut d);
        assert!(hex_eq(&d, b"cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"));
    }
