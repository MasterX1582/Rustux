#pragma once
/* irq_work: the harness counts queue requests instead of running the
 * callback asynchronously. */
struct irq_work {
	void (*func)(struct irq_work *work);
	unsigned long queued;
};
