// klib::prb tests — caller-owned desc/info/text rings, index model.
// Geometry for most tests: descbits=2 (4 descriptors), textbits=6
// (64-byte data ring). `fixture` builds a `Prb` like the C
// `prb_init(rb, text, 6, descs, 2, infos)`.

use std::vec::Vec;

fn fixture<'a>(
    text: &'a mut [u8],
    descs: &'a mut [PrbDesc],
    infos: &'a mut [PrintkInfo],
) -> Prb<'a> {
    Prb::new(text, 6, descs, 2, infos)
}

/// Reserve + fill + commit a record. `final_` picks final_commit over
/// commit; returns the commit handle on success.
fn push(
    prb: &mut Prb,
    msg: &[u8],
    caller_id: u32,
    final_: bool,
) -> Option<PrbReservedEntry> {
    let mut e = PrbReservedEntry::default();
    let mut r = PrintkRecord::wr(msg.len() as u32);
    if !prb.reserve(&mut e, &mut r) {
        return None;
    }
    if let Some(buf) = prb.text_buf_mut(&r) {
        buf[..msg.len()].copy_from_slice(msg);
    }
    {
        let info = prb.info_mut(&r).unwrap();
        info.text_len = msg.len() as u16;
        info.caller_id = caller_id;
        info.ts_nsec = 42;
    }
    if final_ {
        prb.final_commit(&e);
    } else {
        prb.commit(&e);
    }
    Some(e)
}

fn read(prb: &mut Prb, seq: u64) -> Option<(PrintkInfo, Vec<u8>)> {
    let mut info = PrintkInfo::default();
    let mut buf = [0u8; 64];
    if prb.read_valid(seq, Some(&mut info), Some(&mut buf)) {
        let n = (info.text_len as usize).min(buf.len());
        Some((info, buf[..n].to_vec()))
    } else {
        None
    }
}

#[test]
fn init_empty_state() {
    let mut text = [0u8; 64];
    let mut descs = [PrbDesc::default(); 4];
    let mut infos = [PrintkInfo::default(); 4];
    let mut prb = fixture(&mut text, &mut descs, &mut infos);
    assert_eq!(prb.first_seq(), 0);
    assert_eq!(prb.first_valid_seq(), 0);
    assert_eq!(prb.next_seq(), 0);
    assert_eq!(prb.fail, 0);
    assert!(!prb.nrs_livelock());
}

#[test]
fn reserve_commit_read_roundtrip() {
    let mut text = [0u8; 64];
    let mut descs = [PrbDesc::default(); 4];
    let mut infos = [PrintkInfo::default(); 4];
    let mut prb = fixture(&mut text, &mut descs, &mut infos);
    assert!(push(&mut prb, b"hello", 0x77, true).is_some());
    let (info, data) = read(&mut prb, 0).expect("seq 0 readable");
    assert_eq!(info.seq, 0);
    assert_eq!(info.caller_id, 0x77);
    assert_eq!(info.text_len, 5);
    assert_eq!(&data[..], b"hello");
    assert_eq!(prb.first_valid_seq(), 0);
    assert_eq!(prb.next_seq(), 1);
}

#[test]
fn multi_record_ordering() {
    let mut text = [0u8; 64];
    let mut descs = [PrbDesc::default(); 4];
    let mut infos = [PrintkInfo::default(); 4];
    let mut prb = fixture(&mut text, &mut descs, &mut infos);
    assert!(push(&mut prb, b"aaa", 1, true).is_some());
    assert!(push(&mut prb, b"bbb", 2, true).is_some());
    assert!(push(&mut prb, b"ccc", 3, true).is_some());
    /* prb_for_each_record-style iteration: s -> info.seq + 1 */
    let mut s = prb.first_valid_seq();
    let mut got = Vec::new();
    while let Some((info, data)) = read(&mut prb, s) {
        got.push((info.seq, data));
        s = info.seq + 1;
    }
    assert_eq!(got.len(), 3);
    assert_eq!(got[0].0, 0);
    assert_eq!(&got[0].1[..], b"aaa");
    assert_eq!(got[1].0, 1);
    assert_eq!(&got[1].1[..], b"bbb");
    assert_eq!(got[2].0, 2);
    assert_eq!(&got[2].1[..], b"ccc");
}

#[test]
fn wraparound_evicts_oldest() {
    /* tiny rings: 4 descs, 32-byte data */
    let mut text = [0u8; 32];
    let mut descs = [PrbDesc::default(); 4];
    let mut infos = [PrintkInfo::default(); 4];
    let mut prb = Prb::new(&mut text, 5, &mut descs, 2, &mut infos);
    for i in 0..8u64 {
        let msg = [b'0' + i as u8; 8];
        assert!(push(&mut prb, &msg, 1, true).is_some(), "record {} reserved", i);
    }
    /* a stale seq skips forward to the oldest live record */
    let (info, _d) = read(&mut prb, 0).expect("skip forward to tail");
    assert!(info.seq > 0, "seq 0 reclaimed, landed on {}", info.seq);
    assert_eq!(info.seq, prb.first_valid_seq());
    assert_eq!(prb.next_seq(), 8);
    /* newest record still reads back */
    let (_i, data) = read(&mut prb, 7).expect("seq 7 live");
    assert_eq!(&data[..], &[b'7'; 8][..]);
}

#[test]
fn reserve_in_last_appends() {
    let mut text = [0u8; 64];
    let mut descs = [PrbDesc::default(); 4];
    let mut infos = [PrintkInfo::default(); 4];
    let mut prb = fixture(&mut text, &mut descs, &mut infos);
    assert!(push(&mut prb, b"ab", 0x77, false).is_some()); /* commit, not final */
    let mut e = PrbReservedEntry::default();
    let mut r = PrintkRecord::wr(2);
    assert!(prb.reserve_in_last(&mut e, &mut r, 0x77, 64));
    /* the writable region covers old + appended text */
    assert_eq!(r.text_buf_size, 4);
    {
        let buf = prb.text_buf_mut(&r).unwrap();
        assert_eq!(&buf[..2], b"ab");
        buf[2..4].copy_from_slice(b"cd");
    }
    prb.info_mut(&r).unwrap().text_len = 4;
    prb.final_commit(&e);
    let (info, data) = read(&mut prb, 0).expect("extended record");
    assert_eq!(info.text_len, 4);
    assert_eq!(&data[..], b"abcd");
}

#[test]
fn reserve_in_last_wrong_caller_fails() {
    let mut text = [0u8; 64];
    let mut descs = [PrbDesc::default(); 4];
    let mut infos = [PrintkInfo::default(); 4];
    let mut prb = fixture(&mut text, &mut descs, &mut infos);
    assert!(push(&mut prb, b"ab", 0x77, false).is_some());
    let mut e = PrbReservedEntry::default();
    let mut r = PrintkRecord::wr(4);
    assert!(!prb.reserve_in_last(&mut e, &mut r, 0x22, 64));
    assert!(r.info.is_none() && r.text_buf.is_none());
}

#[test]
fn reserve_in_last_finalized_head_fails() {
    let mut text = [0u8; 64];
    let mut descs = [PrbDesc::default(); 4];
    let mut infos = [PrintkInfo::default(); 4];
    let mut prb = fixture(&mut text, &mut descs, &mut infos);
    assert!(push(&mut prb, b"ab", 0x77, true).is_some()); /* final_commit */
    let mut e = PrbReservedEntry::default();
    let mut r = PrintkRecord::wr(4);
    /* head is finalized, not committed: desc_reopen_last refuses */
    assert!(!prb.reserve_in_last(&mut e, &mut r, 0x77, 64));
}

#[test]
fn reserve_size_boundary() {
    let mut text = [0u8; 64];
    let mut descs = [PrbDesc::default(); 4];
    let mut infos = [PrintkInfo::default(); 4];
    let mut prb = fixture(&mut text, &mut descs, &mut infos);
    /* 64B data ring: to_blk_size(24) = 32 <= 32 ok; 25 -> 40 > 32 fails */
    let mut e = PrbReservedEntry::default();
    let mut r = PrintkRecord::wr(25);
    assert!(!prb.reserve(&mut e, &mut r));
    assert!(r.info.is_none() && r.text_buf.is_none());
    let msg = [b'x'; 24];
    assert!(push(&mut prb, &msg, 1, true).is_some());
    let (_i, data) = read(&mut prb, 0).expect("24-byte record");
    assert_eq!(data.len(), 24);
    assert_eq!(&data[..], &msg[..]);
}

#[test]
fn desc_exhaustion_fails_reserve() {
    let mut text = [0u8; 64];
    let mut descs = [PrbDesc::default(); 4];
    let mut infos = [PrintkInfo::default(); 4];
    let mut prb = fixture(&mut text, &mut descs, &mut infos);
    /* three open (uncommitted) reservations */
    let mut es = Vec::new();
    let mut rs = Vec::new();
    for _ in 0..3 {
        let mut e = PrbReservedEntry::default();
        let mut r = PrintkRecord::wr(4);
        assert!(prb.reserve(&mut e, &mut r));
        es.push(e);
        rs.push(r);
    }
    /* the 4th needs the tail pushed past an open desc -> fails */
    let mut e4 = PrbReservedEntry::default();
    let mut r4 = PrintkRecord::wr(4);
    assert!(!prb.reserve(&mut e4, &mut r4));
    assert_eq!(prb.fail, 1);
    /* committing the open records unblocks the ring */
    for e in &es {
        prb.final_commit(e);
    }
    let mut e = PrbReservedEntry::default();
    let mut r = PrintkRecord::wr(4);
    assert!(prb.reserve(&mut e, &mut r));
}

#[test]
fn dataless_record_roundtrip() {
    let mut text = [0u8; 64];
    let mut descs = [PrbDesc::default(); 4];
    let mut infos = [PrintkInfo::default(); 4];
    let mut prb = fixture(&mut text, &mut descs, &mut infos);
    let mut e = PrbReservedEntry::default();
    let mut r = PrintkRecord::wr(0); /* size 0 -> EMPTY_LINE block */
    assert!(prb.reserve(&mut e, &mut r));
    assert!(r.text_buf.is_none());
    prb.info_mut(&r).unwrap().text_len = 0;
    prb.final_commit(&e);
    let (info, _d) = read(&mut prb, 0).expect("data-less record");
    assert_eq!(info.seq, 0);
    assert_eq!(info.text_len, 0);
    /* an empty record counts one (empty) line */
    let mut lc = 0u32;
    let mut i2 = PrintkInfo::default();
    assert!(prb.read_valid_info(0, &mut i2, &mut lc));
    assert_eq!(lc, 1);
}

#[test]
fn read_valid_info_line_count() {
    let mut text = [0u8; 64];
    let mut descs = [PrbDesc::default(); 4];
    let mut infos = [PrintkInfo::default(); 4];
    let mut prb = fixture(&mut text, &mut descs, &mut infos);
    assert!(push(&mut prb, b"a\nb\nc", 1, true).is_some());
    let mut lc = 0u32;
    let mut info = PrintkInfo::default();
    assert!(prb.read_valid_info(0, &mut info, &mut lc));
    assert_eq!(lc, 3);
    assert_eq!(info.text_len, 5);
}

#[test]
fn invalid_seq_and_cleared_ring() {
    let mut text = [0u8; 64];
    let mut descs = [PrbDesc::default(); 4];
    let mut infos = [PrintkInfo::default(); 4];
    let mut prb = fixture(&mut text, &mut descs, &mut infos);
    /* far-future seq: nothing to skip to */
    assert!(read(&mut prb, 77).is_none());
    assert!(push(&mut prb, b"zz", 1, true).is_some());
    assert!(read(&mut prb, 0).is_some());
    prb.clear_ringbuffer();
    assert_eq!(prb.first_valid_seq(), 0);
    assert_eq!(prb.next_seq(), 0);
    assert!(read(&mut prb, 0).is_none());
    assert_eq!(prb.fail, 0);
}

#[test]
fn next_reserve_seq_tracks_head() {
    let mut text = [0u8; 64];
    let mut descs = [PrbDesc::default(); 4];
    let mut infos = [PrintkInfo::default(); 4];
    let mut prb = fixture(&mut text, &mut descs, &mut infos);
    assert!(push(&mut prb, b"aa", 1, true).is_some());
    assert!(push(&mut prb, b"bb", 1, true).is_some());
    /* sane state: Some, equal to next_seq */
    assert_eq!(prb.next_reserve_seq(), Some(prb.next_seq()));
}

#[test]
fn panic_knob_gates() {
    let mut text = [0u8; 64];
    let mut descs = [PrbDesc::default(); 4];
    let mut infos = [PrintkInfo::default(); 4];
    let mut prb = fixture(&mut text, &mut descs, &mut infos);
    assert!(!prb.panic_on_this_cpu());
    prb.panic = true;
    /* sane ring: not livelocked, so panic reports true */
    assert!(!prb.nrs_livelock());
    assert!(prb.panic_on_this_cpu());
    prb.panic = false;
    assert!(!prb.panic_on_this_cpu());
}

#[test]
fn corrupt_record_fields_warn_and_none() {
    // crafted PrintkRecord: info index and text_buf beyond the rings
    // warn + return None instead of panicking on the index.
    let mut text = [0u8; 64];
    let mut descs = [PrbDesc::default(); 4];
    let mut infos = [PrintkInfo::default(); 4];
    let mut prb = fixture(&mut text, &mut descs, &mut infos);
    let mut r = PrintkRecord::wr(4);
    r.info = Some(9);
    assert!(prb.info_mut(&r).is_none());
    r.info = None;
    r.text_buf = Some(63); /* 63 + 4 spills the 64-byte ring */
    assert!(prb.text_buf_mut(&r).is_none());
    r.text_buf = Some(usize::MAX - 8); /* checked_add overflow */
    assert!(prb.text_buf_mut(&r).is_none());
    assert_eq!(prb.warn, 3);
}

#[test]
#[should_panic(expected = "usize::BITS")]
fn init_rejects_wild_geometry() {
    // geometry asserts run before the shifts: a wild descbits is a
    // contract violation, not a shift-overflow panic.
    let mut text = [0u8; 64];
    let mut descs = [PrbDesc::default(); 4];
    let mut infos = [PrintkInfo::default(); 4];
    Prb::new(&mut text, 6, &mut descs, usize::BITS, &mut infos);
}

#[test]
fn first_seq_bounded_on_corrupt_tail() {
    // a tail id whose descriptor never reaches FINALIZED/REUSABLE
    // would spin the upstream retry loop forever; the sequential
    // model bounds it to the ring size and warns.
    let mut text = [0u8; 64];
    let mut descs = [PrbDesc::default(); 4];
    let mut infos = [PrintkInfo::default(); 4];
    let mut prb = fixture(&mut text, &mut descs, &mut infos);
    prb.desc_ring.tail_id = u64::MAX;
    assert_eq!(prb.first_seq(), 0);
    assert!(prb.warn > 0);
}

#[test]
fn crafted_ring_geometry_no_panic() {
    // public ring fields are caller-owned state: wild count_bits /
    // size_bits / ids must degrade to bounded reads, not shift panics
    // or OOB indexes.
    let mut text = [0u8; 64];
    let mut descs = [PrbDesc::default(); 4];
    let mut infos = [PrintkInfo::default(); 4];
    let mut prb = fixture(&mut text, &mut descs, &mut infos);
    prb.desc_ring.count_bits = 63;
    prb.text_ring.size_bits = 63;
    prb.desc_ring.head_id = u64::MAX - 3;
    prb.text_ring.tail_lpos = u64::MAX - 4;
    let _ = prb.first_valid_seq();
    let _ = prb.read_valid(0, None, None);
    let _ = prb.next_seq();
    let _ = prb.first_seq();
}

#[test]
fn block_id_arithmetic_wrapped() {
    // blk begin/next ids near u64::MAX: +7/-1 arithmetic must wrap
    // like the C u64s, not overflow-panic.
    let mut text = [0u8; 64];
    let mut descs = [PrbDesc::default(); 4];
    let mut infos = [PrintkInfo::default(); 4];
    let mut prb = fixture(&mut text, &mut descs, &mut infos);
    prb.text_ring.head_lpos = u64::MAX - 2;
    prb.text_ring.tail_lpos = u64::MAX - 4;
    let mut e = PrbReservedEntry::default();
    let mut r = PrintkRecord::default();
    let _ = prb.reserve(&mut e, &mut r);
    let _ = prb.next_seq();
}
