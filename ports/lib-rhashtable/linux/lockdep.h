#pragma once
/* Lockdep stubs: no lock classes in the harness build. */
struct lock_class_key {
	int _unused;
};

struct lockdep_map {
	/* empty like !CONFIG_LOCKDEP upstream */
};

#define lockdep_init_map(map, name, key, sub)	do { } while (0)
#define lockdep_set_class(lock, key)		do { } while (0)
#define lockdep_is_held(lock)			1
#define lock_acquire_exclusive(map, sub, try, read, ip)	do { } while (0)
#define lock_map_release(map)			do { } while (0)
#define lock_map_acquire(map)			do { } while (0)
