    use super::*;

    #[test]
    fn init_rounds_down_and_reports() {
        let mut buf = [0u8; 40];
        let f = Kfifo::init(&mut buf, 1).unwrap();
        // size 40 -> rounddown pow2 = 32, mask 31
        assert_eq!(kfifo_size(&f), 32);
        assert!(kfifo_is_empty(&f));
        assert_eq!(kfifo_avail(&f, 0), 32);
        assert!(kfifo_initialized(&f));
        assert_eq!(kfifo_esize(&f), 1);
    }

    #[test]
    fn init_small_fails() {
        let mut buf = [0u8; 1];
        assert_eq!(Kfifo::init(&mut buf, 1).err(), Some(-EINVAL));
    }

    #[test]
    fn alloc_node_rounds_up() {
        let mut buf = [0u8; 64];
        let f = Kfifo::alloc_node(&mut buf, 40, 1).unwrap();
        assert_eq!(kfifo_size(&f), 64); // roundup(40)=64
        let mut short = [0u8; 32];
        assert_eq!(Kfifo::alloc_node(&mut short, 40, 1).err(), Some(-ENOMEM));
        let mut one = [0u8; 1];
        assert_eq!(Kfifo::alloc_node(&mut one, 1, 1).err(), Some(-EINVAL));
    }

    #[test]
    fn in_out_basic() {
        let mut buf = [0u8; 8];
        let mut f = Kfifo::init(&mut buf, 1).unwrap();
        assert_eq!(kfifo_in(&mut f, b"abcde", 5), 5);
        assert_eq!(kfifo_len(&f), 5);
        assert_eq!(kfifo_avail(&f, 0), 3);
        let mut out = [0u8; 8];
        assert_eq!(kfifo_out_peek(&mut f, &mut out, 3), 3);
        assert_eq!(&out[..3], b"abc");
        assert_eq!(kfifo_len(&f), 5); // peek doesn't consume
        assert_eq!(kfifo_out(&mut f, &mut out, 10), 5); // drains only 5
        assert_eq!(&out[..5], b"abcde");
        assert!(kfifo_is_empty(&f));
        assert_eq!(kfifo_out(&mut f, &mut out, 1), 0);
    }

    #[test]
    fn wraparound_ring() {
        let mut buf = [0u8; 8];
        let mut f = Kfifo::init(&mut buf, 1).unwrap();
        let mut out = [0u8; 4];
        // Fill 6, drain 4, fill 6 (wraps), drain all.
        assert_eq!(kfifo_in(&mut f, b"abcdef", 6), 6);
        assert_eq!(kfifo_out(&mut f, &mut out, 4), 4);
        assert_eq!(&out, b"abcd");
        assert_eq!(kfifo_in(&mut f, b"ghijkl", 6), 6);
        assert_eq!(kfifo_len(&f), 8);
        assert!(kfifo_is_full(&f));
        assert_eq!(kfifo_in(&mut f, b"xx", 2), 0); // full
        let mut all = [0u8; 8];
        assert_eq!(kfifo_out(&mut f, &mut all, 8), 8);
        assert_eq!(&all, b"efghijkl");
    }

    #[test]
    fn counter_overflow_wrap() {
        let mut buf = [0u8; 8];
        let mut f = Kfifo::init(&mut buf, 1).unwrap();
        f.in_ = 0xfffffffc;
        f.out = 0xfffffffc;
        assert_eq!(kfifo_in(&mut f, b"abcd", 4), 4);
        // in_ wrapped to 0
        assert_eq!(f.in_, 0);
        assert_eq!(kfifo_len(&f), 4);
        let mut out = [0u8; 4];
        assert_eq!(kfifo_out(&mut f, &mut out, 4), 4);
        assert_eq!(&out, b"abcd");
        assert_eq!(f.out, 0);
    }

    #[test]
    fn out_linear_run() {
        let mut buf = [0u8; 8];
        let mut f = Kfifo::init(&mut buf, 1).unwrap();
        kfifo_in(&mut f, b"abcdef", 6);
        let mut tmp = [0u8; 8];
        kfifo_out(&mut f, &mut tmp, 4); // out=4
        kfifo_in(&mut f, b"ghij", 4); // wraps: data = ghij + ef at end
        // remaining 6 elems: out=4 -> off=4, contiguous run = size-off = 4
        let (cnt, off) = kfifo_out_linear(&f, 10);
        assert_eq!(off, 4);
        assert_eq!(cnt, 4); // min(10, 6, 8-4)
        assert_eq!(&f.data[4..8], b"efgh");
    }

    #[test]
    fn record_fifo_recsize1() {
        let mut buf = [0u8; 64];
        let mut f = Kfifo::init(&mut buf, 1).unwrap();
        // recsize 1: each record costs len + 1 byte header
        assert_eq!(kfifo_in_r(&mut f, b"hello", 5, 1), 5);
        assert_eq!(kfifo_in_r(&mut f, b"world!", 6, 1), 6);
        assert_eq!(kfifo_len_r(&f, 1), 5); // first record is 5
        let mut out = [0u8; 8];
        assert_eq!(kfifo_out_peek_r(&mut f, &mut out, 8, 1), 5);
        assert_eq!(&out[..5], b"hello");
        assert_eq!(kfifo_out_r(&mut f, &mut out, 8, 1), 5);
        // next record
        assert_eq!(kfifo_len_r(&f, 1), 6);
        kfifo_skip_r(&mut f, 1); // skip whole record without copying
        assert!(kfifo_is_empty(&f));
    }

    #[test]
    fn esize_element_addressing() {
        // esize=2: elements are u16; buf is byte-view in the model.
        let mut buf = [0u8; 16];
        let mut f = Kfifo::init(&mut buf, 2).unwrap();
        let src = [0x34u8, 0x12, 0x78, 0x56]; // two u16 elements
        assert_eq!(kfifo_in(&mut f, &src, 2), 2); // 2 ELEMENTS = 4 bytes
        assert_eq!(kfifo_len(&f), 2);
        let mut out = [0u8; 4];
        assert_eq!(kfifo_out(&mut f, &mut out, 2), 2);
        assert_eq!(&out, &src);
    }

    #[test]
    fn from_to_user_paths() {
        let mut buf = [0u8; 16];
        let mut f = Kfifo::init(&mut buf, 1).unwrap();
        let (ret, n) = kfifo_from_user(&mut f, b"xyz123", 6);
        assert_eq!((ret, n), (0, 6));
        let mut dst = [0u8; 6];
        let (ret, n) = kfifo_to_user(&mut f, &mut dst, 6);
        assert_eq!((ret, n), (0, 6));
        assert_eq!(&dst, b"xyz123");
    }

    #[test]
    fn dma_prepare_segments() {
        let mut buf = [0u8; 16];
        let mut f = Kfifo::init(&mut buf, 1).unwrap();
        let mut sgl = [Sg::default(); 4];
        sg_init_table(&mut sgl);
        // DMA in: 8 bytes at off 0 -> exactly one segment
        let n = kfifo_dma_in_prepare(&mut f, &mut sgl, 4, 8, 0);
        assert_eq!(n, 1);
        assert_eq!(sgl[0].off, 0);
        assert_eq!(sgl[0].length, 8);
        assert_eq!(sgl[0].dma_address, 0);
        // simulate device writing into the sg target, then finish
        kfifo_dma_in_finish(&mut f, 8, 0);
        assert_eq!(kfifo_len(&f), 8);
        let n2 = kfifo_dma_out_prepare(&mut f, &mut sgl, 4, 4, 0);
        assert_eq!(n2, 1);
        assert_eq!(sgl[0].off, 0);
        assert_eq!(sgl[0].length, 4);
    }

    #[test]
    fn reset_paths() {
        let mut buf = [0u8; 8];
        let mut f = Kfifo::init(&mut buf, 1).unwrap();
        kfifo_in(&mut f, b"abc", 3);
        kfifo_reset_out(&mut f);
        assert!(kfifo_is_empty(&f));
        assert_eq!(f.out, 3);
        kfifo_in(&mut f, b"de", 2);
        kfifo_reset(&mut f);
        assert_eq!(f.in_, 0);
        assert_eq!(f.out, 0);
    }

    #[test]
    fn max_r_clamps_record_len() {
        // recsize 1 -> u8 header max 255; recsize 2 -> u16 max 65535
        assert_eq!(kfifo_max_r(300, 1), 255);
        assert_eq!(kfifo_max_r(300, 2), 300);
        assert_eq!(kfifo_max_r(100000, 2), 65535);
    }

    #[test]
    fn element_put_get_peek() {
        let mut buf = [0u8; 8];
        let mut f = Kfifo::init(&mut buf, 1).unwrap();
        assert_eq!(kfifo_get(&mut f, &mut [0u8; 1], 0), 0);
        assert_eq!(kfifo_put(&mut f, b"x", 0), 1);
        assert_eq!(kfifo_put(&mut f, b"y", 0), 1);
        let mut v = [0u8; 1];
        assert_eq!(kfifo_peek(&f, &mut v, 0), 1);
        assert_eq!(v[0], b'x');
        assert_eq!(kfifo_len(&f), 2); // peek doesn't consume
        assert_eq!(kfifo_get(&mut f, &mut v, 0), 1);
        assert_eq!(v[0], b'x');
        assert_eq!(kfifo_len(&f), 1);
        // peek_len on a plain fifo returns bytes = len * esize
        assert_eq!(kfifo_peek_len(&f, 0), 1);
        // fill to full -> put fails
        for _ in 0..7 {
            kfifo_put(&mut f, b"z", 0);
        }
        assert!(kfifo_is_full(&f));
        assert_eq!(kfifo_put(&mut f, b"w", 0), 0);
    }

    #[test]
    fn record_avail_and_peek_len() {
        let mut buf = [0u8; 32];
        let mut f = Kfifo::init(&mut buf, 1).unwrap();
        // recsize=1: avail accounts for the next record's 1-byte header
        assert_eq!(kfifo_avail(&f, 1), 31); // 32 free - 1 header
        kfifo_in_r(&mut f, b"abc", 3, 1);
        assert_eq!(kfifo_peek_len(&f, 1), 3); // first record is 3
        assert_eq!(kfifo_avail(&f, 1), 27); // 32 - 4 used - 1 header
        kfifo_skip_r(&mut f, 1);
        assert!(kfifo_is_empty(&f));
    }

    #[test]
    fn record_recsize2_headers() {
        let mut buf = [0u8; 64];
        let mut f = Kfifo::init(&mut buf, 1).unwrap();
        // recsize 2: two-byte little-endian length header
        assert_eq!(kfifo_in_r(&mut f, b"payload", 7, 2), 7);
        assert_eq!(kfifo_len_r(&f, 2), 7);
        // header bytes: 0x07 0x00 little-endian
        assert_eq!(f.data[0], 7);
        assert_eq!(f.data[1], 0);
        let mut out = [0u8; 16];
        assert_eq!(kfifo_out_r(&mut f, &mut out, 16, 2), 7);
        assert_eq!(&out[..7], b"payload");
    }

    #[test]
    fn record_dma_path() {
        let mut buf = [0u8; 32];
        let mut f = Kfifo::init(&mut buf, 1).unwrap();
        let mut sgl = [Sg::default(); 4];
        sg_init_table(&mut sgl);
        // record fifo DMA: len is clamped by max_r, header included
        let n = kfifo_dma_in_prepare_r(&mut f, &mut sgl, 4, 6, 1, 0x1000);
        assert_eq!(n, 1);
        kfifo_dma_in_finish_r(&mut f, 6, 1);
        assert_eq!(kfifo_len_r(&f, 1), 6);
        let n2 = kfifo_dma_out_prepare_r(&mut f, &mut sgl, 4, 3, 1, 0x2000);
        assert_eq!(n2, 1);
        kfifo_skip_r(&mut f, 1);
    }

    #[test]
    fn dma_two_segment_wrap() {
        let mut buf = [0u8; 8];
        let mut f = Kfifo::init(&mut buf, 1).unwrap();
        kfifo_in(&mut f, b"abcdef", 6);
        let mut tmp = [0u8; 8];
        kfifo_out(&mut f, &mut tmp, 4); // out=4
        // DMA-in 6 at in=6: contiguous to end is 2 -> 2 segments (2 + 4)
        let mut sgl = [Sg::default(); 4];
        sg_init_table(&mut sgl);
        let n = kfifo_dma_in_prepare(&mut f, &mut sgl, 4, 6, 0);
        assert_eq!(n, 2);
        assert_eq!(sgl[0].off, 6);
        assert_eq!(sgl[0].length, 2);
        assert_eq!(sgl[1].off, 0);
        assert_eq!(sgl[1].length, 4);
        // nents=1 limits to the first segment
        let n1 = kfifo_dma_in_prepare(&mut f, &mut sgl, 1, 6, 0);
        assert_eq!(n1, 1);
    }

    #[test]
    fn record_user_paths() {
        let mut buf = [0u8; 64];
        let mut f = Kfifo::init(&mut buf, 1).unwrap();
        let (ret, n) = kfifo_from_user_r(&mut f, b"user", 4, 1);
        assert_eq!((ret, n), (0, 4));
        assert_eq!(kfifo_len_r(&f, 1), 4);
        let mut dst = [0u8; 8];
        let (ret, n) = kfifo_to_user_r(&mut f, &mut dst, 8, 1);
        assert_eq!((ret, n), (0, 4));
        assert_eq!(&dst[..4], b"user");
    }

    #[test]
    fn dispatch_layer() {
        let mut buf = [0u8; 32];
        let mut f = Kfifo::init(&mut buf, 1).unwrap();
        // plain arm of skip_count/skip
        kfifo_in(&mut f, b"abcdef", 6);
        kfifo_skip(&mut f, 0);
        assert_eq!(kfifo_len(&f), 5);
        kfifo_skip_count(&mut f, 3, 0);
        assert_eq!(kfifo_len(&f), 2);
        // record arm: skip ignores count, drops whole record
        kfifo_reset(&mut f);
        kfifo_in_r(&mut f, b"rec1", 4, 1);
        kfifo_in_r(&mut f, b"rec2", 4, 1);
        kfifo_skip_count(&mut f, 99, 1); // count ignored for records
        assert_eq!(kfifo_len_r(&f, 1), 4); // next record intact
        // dma_out_finish on a record fifo -> whole-record skip
        kfifo_dma_out_finish(&mut f, 99, 1);
        assert!(kfifo_is_empty(&f));
    }
