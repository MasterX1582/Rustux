#pragma once
/* Minimal bug.h shim for the lib-genradix-interval harness.
 * BUG_ON/BUILD_BUG_ON match upstream semantics: the recorded build is
 * !CONFIG_BUG so BUG_ON is an aborting check kept for the harness and
 * BUILD_BUG_ON is a compile-time assertion. */
#include <stdlib.h>

#define BUG_ON(cond) do { if (cond) abort(); } while (0)
#define BUILD_BUG_ON(cond) ((void)sizeof(char[1 - 2 * !!(cond)]))
