// klib::percpu_counter tests — caller-owned lane slice, sequential model.
// Each test's `lanes` is the per-cpu arena (one cell per cpu); `c.cpu`
// is the ambient smp_processor_id() the this_cpu ops touch.

#[test]
fn init_and_read() {
    let mut lanes = [0i64; 4];
    let mut c = PercpuCounter::new();
    assert!(!c.initialized());
    assert_eq!(c.init(&mut lanes, 7), 0);
    assert!(c.initialized());
    assert_eq!(c.read(), 7);
    assert_eq!(c.read_positive(), 7);
    assert_eq!(c.sum(), 7);
    assert_eq!(c.counters.as_deref().unwrap(), &[0i64; 4][..]);
}

#[test]
fn add_below_batch_stays_in_lane() {
    let mut lanes = [0i64; 4];
    let mut c = PercpuCounter::new();
    c.init(&mut lanes, 0);
    c.batch = 8;
    c.cpu = 1;
    c.add(5);
    /* |lane + amount| < batch: global count untouched */
    assert_eq!(c.read(), 0);
    assert_eq!(c.counters.as_deref().unwrap()[1], 5);
    assert_eq!(c.sum(), 5);
}

#[test]
fn add_crossing_batch_folds() {
    let mut lanes = [0i64; 4];
    let mut c = PercpuCounter::new();
    c.init(&mut lanes, 0);
    c.batch = 8;
    c.cpu = 1;
    c.add(5);
    c.add(4); /* 5 + 4 = 9 >= batch: fold lane into count */
    assert_eq!(c.read(), 9);
    assert_eq!(c.counters.as_deref().unwrap()[1], 0);
    assert_eq!(c.sum(), 9);
}

#[test]
fn sum_across_lanes() {
    let mut lanes = [0i64; 4];
    let mut c = PercpuCounter::new();
    c.init(&mut lanes, 100);
    c.batch = 32;
    for cpu in 0..4 {
        c.cpu = cpu;
        c.add(10); /* each stays lane-local (< 32) */
    }
    assert_eq!(c.read(), 100); /* global untouched */
    assert_eq!(c.sum(), 140); /* lanes counted by sum */
    let l = c.counters.as_deref().unwrap();
    assert!(l.iter().all(|&v| v == 10));
}

#[test]
fn set_clears_all_lanes() {
    let mut lanes = [0i64; 4];
    let mut c = PercpuCounter::new();
    c.init(&mut lanes, 0);
    c.batch = 32;
    c.cpu = 0;
    c.add(10);
    c.cpu = 2;
    c.add(20);
    c.set(-50);
    assert_eq!(c.read(), -50);
    assert_eq!(c.read_positive(), 0);
    assert_eq!(c.sum(), -50);
    assert!(c.counters.as_deref().unwrap().iter().all(|&v| v == 0));
}

#[test]
fn sync_folds_this_cpu() {
    let mut lanes = [0i64; 4];
    let mut c = PercpuCounter::new();
    c.init(&mut lanes, 0);
    c.batch = 32;
    c.cpu = 3;
    c.add(17);
    assert_eq!(c.read(), 0);
    c.sync();
    assert_eq!(c.read(), 17);
    assert_eq!(c.counters.as_deref().unwrap()[3], 0);
    assert_eq!(c.sum(), 17);
}

#[test]
fn inc_dec_sub_and_returns() {
    let mut lanes = [0i64; 4];
    let mut c = PercpuCounter::new();
    c.init(&mut lanes, 0);
    c.batch = 8;
    c.cpu = 0;
    c.inc();
    c.inc();
    c.dec();
    assert_eq!(c.read(), 0); /* still lane-local */
    assert_eq!(c.sum(), 1);
    /* add_return reports the precise sum after the op */
    assert_eq!(c.add_return(30), 31);
    assert_eq!(c.read(), 31); /* 30 crossed batch: folded */
    assert_eq!(c.sub_return(20), 11);
    assert_eq!(c.read(), 11);
    c.sub(3);
    assert_eq!(c.sum(), 8);
}

#[test]
fn local_batch_never_folds() {
    let mut lanes = [0i64; 4];
    let mut c = PercpuCounter::new();
    c.init(&mut lanes, 0);
    c.cpu = 0;
    c.add_local(1 << 20); /* PERCPU_COUNTER_LOCAL_BATCH = i32::MAX */
    assert_eq!(c.read(), 0);
    assert_eq!(c.sum(), 1 << 20);
}

#[test]
fn compare_rough_and_precise() {
    let mut lanes = [0i64; 4];
    let mut c = PercpuCounter::new();
    c.init(&mut lanes, 0);
    c.batch = 8;
    /* rough path: |count - rhs| > batch*ncpu (32) -> no sum */
    c.set(1000);
    assert_eq!(c.compare(0), 1);
    assert_eq!(c.compare(2000), -1);
    /* precise path: lane deviations are summed */
    c.set(0);
    c.cpu = 0;
    c.add(5); /* lane-local only */
    assert_eq!(c.compare(0), 1); /* sum() = 5 > 0 */
    assert_eq!(c.compare(5), 0);
    assert_eq!(c.compare(6), -1);
}

#[test]
fn limited_add_bounds() {
    let mut lanes = [0i64; 4];
    let mut c = PercpuCounter::new();
    c.init(&mut lanes, 0);
    c.batch = 8;
    c.cpu = 0;
    c.set(100);
    /* fast path: |0+1| <= 8 and 100 + 32 <= 200 -> lane, no lock */
    assert!(c.limited_add(200, 1));
    assert_eq!(c.read(), 100);
    /* slow path reject: 100 + 10 + lanes(1) = 111 > 105 */
    assert!(!c.limited_add(105, 10));
    /* negative fast path: |1-3| <= 8 and 100 - 32 >= 60 -> lane only */
    assert!(c.limited_add(60, -3));
    assert_eq!(c.read(), 100);
    assert_eq!(c.sum(), 98);
    /* slow path negative: 100 - 32 >= 50 -> folds lane + amount */
    assert!(c.limited_add(50, -10));
    assert_eq!(c.read(), 88); /* 100 + (-2) + (-10) folded */
    assert_eq!(c.sum(), 88);
    /* slow path reject: 88 - 10 + lanes(0) = 78 < 85 */
    assert!(!c.limited_add(85, -10));
    assert_eq!(c.sum(), 88);
    /* amount == 0 always succeeds */
    assert!(c.limited_add(0, 0));
}

#[test]
fn destroy_returns_lanes_and_nulls() {
    let mut lanes = [0i64; 4];
    let mut c = PercpuCounter::new();
    c.init(&mut lanes, 5);
    let back = c.destroy().expect("live counter returns its lanes");
    assert!(!c.initialized());
    assert_eq!(back.len(), 4);
    /* the returned arena is a fresh borrow: usable for re-init */
    assert_eq!(c.init(back, 42), 0);
    assert_eq!(c.sum(), 42);
}

#[test]
#[should_panic(expected = "uninitialized counter")]
fn add_on_destroyed_panics() {
    let mut lanes = [0i64; 4];
    let mut c = PercpuCounter::new();
    c.init(&mut lanes, 0);
    c.destroy();
    c.add(1);
}

#[test]
fn fail_mask_fails_init() {
    let mut lanes = [0i64; 4];
    let mut c = PercpuCounter::new();
    c.fail_mask = 1; /* fail the first alloc attempt */
    assert_eq!(c.init(&mut lanes, 0), ENOMEM);
    assert!(!c.initialized());
    assert_eq!(c.alloc_ctr, 1);
}

#[test]
fn init_many_shares_alloc() {
    let mut lanes = [0i64; 8];
    let mut fbcs = [PercpuCounter::new(), PercpuCounter::new()];
    assert_eq!(PercpuCounter::init_many(&mut fbcs, &mut lanes, 4, 3), 0);
    assert!(fbcs[0].initialized() && fbcs[1].initialized());
    fbcs[1].cpu = 2;
    fbcs[1].batch = 8;
    fbcs[1].add(5);
    /* disjoint chunks: counter 0 untouched */
    assert_eq!(fbcs[0].sum(), 3);
    assert_eq!(fbcs[0].counters.as_deref().unwrap()[2], 0);
    assert_eq!(fbcs[1].counters.as_deref().unwrap()[2], 5);
    assert_eq!(fbcs[1].sum(), 8);
    PercpuCounter::destroy_many(&mut fbcs);
    assert!(!fbcs[0].initialized() && !fbcs[1].initialized());
}

#[test]
fn init_many_fail_mask() {
    let mut lanes = [0i64; 8];
    let mut fbcs = [PercpuCounter::new(), PercpuCounter::new()];
    fbcs[0].fail_mask = 1;
    assert_eq!(PercpuCounter::init_many(&mut fbcs, &mut lanes, 4, 0), ENOMEM);
    assert!(!fbcs[0].initialized() && !fbcs[1].initialized());
}

#[test]
fn cpu_oob_is_warned_noop() {
    // caller-corrupted `cpu` beyond the lane arena: warn + no-op
    // (the `nidx`-scratch convention), never an index panic.
    let mut lanes = [0i64; 4];
    let mut c = PercpuCounter::new();
    assert_eq!(c.init(&mut lanes, 1), 0);
    c.cpu = 9;
    c.add(5);
    c.sync();
    assert_eq!(c.sum(), 1);
    assert_eq!(c.warn, 2);
    assert_eq!(c.counters.as_deref().unwrap(), &[0i64; 4][..]);
    // limited_add_batch is the other this_cpu consumer
    let mut c2 = PercpuCounter::new();
    let mut lanes2 = [0i64; 4];
    assert_eq!(c2.init(&mut lanes2, 0), 0);
    c2.cpu = usize::MAX;
    assert!(!c2.limited_add_batch(10, 32, 100));
    assert_eq!(c2.warn, 1);
}

#[test]
fn wrapping_sum_and_compare() {
    // i64 arithmetic wraps like C — no overflow panic on crafted lanes.
    let mut lanes = [0i64; 4];
    let mut c = PercpuCounter::new();
    assert_eq!(c.init(&mut lanes, i64::MAX), 0);
    c.counters.as_deref_mut().unwrap()[..2].fill(i64::MAX);
    assert_eq!(c.sum(), i64::MAX - 2); // 3*MAX wraps to MAX-2
    // compare_batch against a wrapped unknown band: no panic
    let _ = c.compare(i64::MAX);
    let _ = c.compare_batch(i64::MAX, 32);
    // sub at i64::MIN negates through wrapping_neg
    c.sub(i64::MIN);
}

#[test]
fn compare_batch_widens_before_multiply() {
    // upstream computes (s64)batch * num_online_cpus; the earlier
    // i32-then-widen version overflowed to a NEGATIVE threshold for
    // batch=i32::MAX, ncpu>=2, flipping compare()s fast-path result.
    let mut lanes = [0i64; 4];
    let mut c = PercpuCounter::new();
    c.init(&mut lanes, 0);
    c.count = i64::MAX;
    // |count - rhs| = 0 <= unknown -> exact-sum path -> equal -> 0.
    // the i32-mul bug made unknown=-4 so it returned -1 early.
    assert_eq!(c.compare_batch(i64::MAX, i32::MAX), 0);
}
