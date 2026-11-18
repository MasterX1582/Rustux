#pragma once
struct work_struct {
	void (*func)(struct work_struct *work);
	unsigned long queued;
};
