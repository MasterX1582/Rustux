use super::*;

fn chain(next: &[usize], first: usize) -> usize {
    let mut n = 0;
    let mut cur = first;
    while cur != NIL {
        n += 1;
        cur = next[cur];
    }
    n
}

fn order(next: &[usize], first: usize) -> [usize; 8] {
    let mut out = [NIL; 8];
    let mut i = 0;
    let mut cur = first;
    while cur != NIL && i < 8 {
        out[i] = cur;
        i += 1;
        cur = next[cur];
    }
    out
}

#[test]
fn empty_head() {
    let h = LlistHead::new();
    let mut next = [0usize; 8];
    assert!(llist_empty(&h));
    assert_eq!(llist_del_first(&mut { h }, &mut next), NIL);
    assert_eq!(llist_del_all(&mut { h }), NIL);
}

#[test]
fn add_pop_lifo() {
    let mut h = LlistHead::new();
    let mut next = [0usize; 8];
    for i in 0..3 {
        init_llist_node(&mut next, i);
    }
    assert!(llist_add(&mut h, &mut next, 0)); // was empty
    assert!(!llist_add(&mut h, &mut next, 1));
    assert!(!llist_add(&mut h, &mut next, 2));
    assert_eq!(order(&next, h.first), [2, 1, 0, NIL, NIL, NIL, NIL, NIL]);
    assert!(llist_on_list(&next, 1));
    assert_eq!(llist_del_first(&mut h, &mut next), 2);
    assert_eq!(llist_del_first(&mut h, &mut next), 1);
    assert_eq!(llist_del_first(&mut h, &mut next), 0);
    assert_eq!(llist_del_first(&mut h, &mut next), NIL);
    assert!(llist_empty(&h));
}

#[test]
fn del_first_this() {
    let mut h = LlistHead::new();
    let mut next = [0usize; 8];
    llist_add(&mut h, &mut next, 4);
    llist_add(&mut h, &mut next, 7);
    assert!(!llist_del_first_this(&mut h, &mut next, 4)); // 7 is first
    assert_eq!(h.first, 7);
    assert!(llist_del_first_this(&mut h, &mut next, 7));
    assert_eq!(h.first, 4);
}

#[test]
fn del_first_init_marks_off_list() {
    let mut h = LlistHead::new();
    let mut next = [0usize; 8];
    init_llist_node(&mut next, 3);
    llist_add(&mut h, &mut next, 3);
    assert!(llist_on_list(&next, 3));
    assert_eq!(llist_del_first_init(&mut h, &mut next), 3);
    assert!(!llist_on_list(&next, 3));
}

#[test]
fn del_all_detaches_chain() {
    let mut h = LlistHead::new();
    let mut next = [0usize; 8];
    for i in 0..4 {
        llist_add(&mut h, &mut next, i);
    }
    let chain = llist_del_all(&mut h);
    assert!(llist_empty(&h));
    assert_eq!(order(&next, chain), [3, 2, 1, 0, NIL, NIL, NIL, NIL]);
    // list can be reused after detach
    llist_add(&mut h, &mut next, 5);
    assert_eq!(h.first, 5);
}

#[test]
fn reverse_order() {
    let mut next = [0usize; 8];
    assert_eq!(llist_reverse_order(&mut next, NIL), NIL);
    next[2] = NIL;
    assert_eq!(llist_reverse_order(&mut next, 2), 2);
    // 0 -> 1 -> 2 -> NIL becomes 2 -> 1 -> 0 -> NIL
    next[0] = 1;
    next[1] = 2;
    next[2] = NIL;
    let head = llist_reverse_order(&mut next, 0);
    assert_eq!(order(&next, head), [2, 1, 0, NIL, NIL, NIL, NIL, NIL]);
}

#[test]
fn add_batch() {
    let mut h = LlistHead::new();
    let mut next = [0usize; 8];
    // batch chain 5 -> 6 -> (whatever head was)
    next[5] = 6;
    assert!(llist_add_batch(&mut h, &mut next, 5, 6));
    assert_eq!(order(&next, h.first), [5, 6, NIL, NIL, NIL, NIL, NIL, NIL]);
    // second batch prepends: 1 -> 2 -> 5 -> 6
    next[1] = 2;
    assert!(!llist_add_batch(&mut h, &mut next, 1, 2));
    assert_eq!(order(&next, h.first), [1, 2, 5, 6, NIL, NIL, NIL, NIL]);
}

#[test]
fn private_variants_match() {
    let mut h = LlistHead::new();
    let mut next = [0usize; 8];
    assert!(__llist_add(&mut h, &mut next, 0));
    next[1] = 2;
    assert!(!__llist_add_batch(&mut h, &mut next, 1, 2));
    assert_eq!(order(&next, h.first), [1, 2, 0, NIL, NIL, NIL, NIL, NIL]);
    assert_eq!(__llist_del_all(&mut h), 1);
    assert!(llist_empty(&h));
}

#[test]
fn interleaved_fuzz() {
    // seeded add/del/reverse sequence; a model set tracks membership
    // because a popped node's `next` still points at the old chain
    // (llist_on_list can't detect that — same in C).
    let mut h = LlistHead::new();
    let mut next = [NIL; 32];
    let mut on = [false; 32];
    let mut model_len = 0usize;
    let mut rng = 0x9e3779b97f4a7c15u64;
    let mut xs = || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        rng
    };
    for _ in 0..500 {
        match xs() % 4 {
            0 => {
                let i = (xs() % 32) as usize;
                if !on[i] {
                    llist_add(&mut h, &mut next, i);
                    on[i] = true;
                    model_len += 1;
                }
            }
            1 => {
                let n = llist_del_first_init(&mut h, &mut next);
                if n != NIL {
                    on[n] = false;
                    model_len -= 1;
                }
            }
            2 => {
                let c = llist_del_all(&mut h);
                h.first = llist_reverse_order(&mut next, c);
            }
            _ => {
                assert_eq!(llist_empty(&h), h.first == NIL);
            }
        }
        assert_eq!(chain(&next, h.first), model_len);
    }
}
