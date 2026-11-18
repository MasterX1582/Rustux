#pragma once
/* Minimal linux/atomic.h shim for the lib-llist harness.
 *
 * The recorded reference is a single-threaded build: the lockless
 * cmpxchg loops in lib/llist.c always succeed on the first pass, so
 * each atomic op is a plain deterministic load/store.
 *
 * Semantics are still the real ones:
 *   smp_load_acquire(p)      -> READ_ONCE(*p)
 *   xchg(p, v)               -> swap *p with v, return the old value
 *   try_cmpxchg(p, oldp, n)  -> if (*p == *oldp) store n into *p and
 *                               return true; else store *p into *oldp
 *                               and return false.  In a single-threaded
 *                               model oldp always holds the value just
 *                               read, so it always succeeds on the
 *                               first try — the loop bodies still see
 *                               the exact C semantics.
 */
#include <linux/types.h>
#include <linux/compiler.h>

#define smp_load_acquire(p) READ_ONCE(*(p))
#define smp_store_release(p, v) WRITE_ONCE(*(p), v)

#define xchg(ptr, v) ({						\
	__typeof__(ptr) _xp = (ptr);				\
	__typeof__(*_xp) _xo = *_xp;				\
	*_xp = (v);						\
	_xo;							\
})

#define try_cmpxchg(ptr, oldp, new) ({				\
	__typeof__(ptr) _cp = (ptr);				\
	__typeof__(oldp) _cop = (oldp);				\
	__typeof__(*_cp) _cold = *_cop;				\
	_Bool _cret;						\
	if (*_cp == _cold) {					\
		*_cp = (new);					\
		_cret = true;					\
	} else {						\
		*_cop = *_cp;					\
		_cret = false;					\
	}							\
	_cret;							\
})

#define cmpxchg(ptr, old, new) ({				\
	__typeof__(ptr) _cp = (ptr);				\
	__typeof__(*_cp) _cold = *_cp;				\
	if (_cold == (old))					\
		*_cp = (new);					\
	_cold;							\
})
