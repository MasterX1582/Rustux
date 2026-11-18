#pragma once
/* Bit spinlocks: they really set/clear bit @bit of the word — the
 * rhashtable bucket lock bit is observable state (rht_ptr masks it). */

static inline void bit_spin_lock(int bitnum, unsigned long *addr)
{
	*addr |= (1UL << bitnum);
}

static inline void bit_spin_unlock(int bitnum, unsigned long *addr)
{
	*addr &= ~(1UL << bitnum);
}

static inline int bit_spin_is_locked(int bitnum, unsigned long *addr)
{
	return !!(*addr & (1UL << bitnum));
}
