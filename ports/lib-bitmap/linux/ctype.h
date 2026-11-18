#pragma once
/* bitmap.c includes linux/ctype.h but uses no classification function
 * (parsing lives in lib/bitmap-str.c upstream). Stub kept minimal so a
 * <ctype.h> passthrough can't loop back to this file under -I linux. */
