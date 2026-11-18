#pragma once
/* linux/device.h shim — just enough for devm_bitmap_alloc/zalloc to
 * compile and link. Actions are recorded in a small static table (the
 * harness never exercises device teardown); on overflow the action runs
 * immediately and -ENOMEM is returned, matching upstream's contract. */
#include <linux/types.h>
#include <linux/errno.h>

struct device {
	int dummy;
};

#define DEVM_MAX_ACTIONS 64

struct devm_action {
	void (*fn)(void *);
	void *data;
};

extern struct devm_action __harness_devm_actions[DEVM_MAX_ACTIONS];
extern int __harness_devm_nactions;

static inline int devm_add_action_or_reset(struct device *dev,
					   void (*action)(void *),
					   void *data)
{
	(void)dev;
	if (__harness_devm_nactions >= DEVM_MAX_ACTIONS) {
		action(data);
		return -ENOMEM;
	}
	__harness_devm_actions[__harness_devm_nactions].fn = action;
	__harness_devm_actions[__harness_devm_nactions].data = data;
	__harness_devm_nactions++;
	return 0;
}
