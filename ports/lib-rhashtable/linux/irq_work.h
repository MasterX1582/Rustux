#pragma once
#include <linux/irq_work_types.h>

#define init_irq_work(w, f)	do { (w)->func = (f); (w)->queued = 0; } while (0)

/* Queuing just marks the work pending; the harness drains it through
 * explicit run_work_now() calls so behaviour stays sequential and
 * deterministic on both sides of the port. */
#define irq_work_queue(w)	do { (w)->queued = 1; } while (0)
#define irq_work_sync(w)	do { } while (0)
