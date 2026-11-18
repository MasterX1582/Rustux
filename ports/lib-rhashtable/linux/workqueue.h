#pragma once
#include <linux/workqueue_types.h>

#define INIT_WORK(w, f)		do { (w)->func = (f); (w)->queued = 0; } while (0)
#define schedule_work(w)	do { (w)->queued = 1; } while (0)
#define cancel_work_sync(w)	do { (w)->queued = 0; } while (0)
#define flush_work(w)		do { (w)->queued = 0; } while (0)
