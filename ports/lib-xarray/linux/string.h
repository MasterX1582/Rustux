#pragma once
/* <string.h> passthrough: no linux/ names collide with libc's, so
 * this is just the real thing for memset/memcpy users in shims.
 * include_next: the record build puts -I linux ahead of the system
 * paths, so a plain <string.h> here would resolve back to this file. */
#include_next <string.h>
