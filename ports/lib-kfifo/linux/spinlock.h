#pragma once
/* Spinlock stubs: the __kfifo_* core never locks; the typed-layer
 * *_spinlocked macros in kfifo.h expand to these no-ops so a single
 * reader+writer harness stays faithful (kfifo.h documents that no
 * locking is required for the one-reader/one-writer case). */

typedef struct { int __unused; } spinlock_t;

#define spin_lock(lock)			do { (void)(lock); } while (0)
#define spin_unlock(lock)		do { (void)(lock); } while (0)
#define spin_lock_irqsave(lock, flags)	do { (void)(lock); (flags) = 0; } while (0)
#define spin_unlock_irqrestore(lock, flags) \
					do { (void)(lock); (void)(flags); } while (0)
