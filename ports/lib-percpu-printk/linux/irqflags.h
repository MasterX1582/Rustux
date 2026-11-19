#pragma once
/* linux/irqflags.h shim: the sequential harness is the only context,
 * so irq save/restore are the flag-word stores upstream's macros do
 * (the saved value is observable garbage-free but unused). */

#define local_irq_save(f) ((f) = 0)
#define local_irq_restore(f) do { (void)(f); } while (0)
#define local_irq_disable() do { } while (0)
#define local_irq_enable() do { } while (0)
