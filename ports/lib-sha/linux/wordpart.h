#pragma once

/* From upstream <linux/wordpart.h>: replicate a byte across a word.
 * LP64: unsigned long is 64 bits, matching the derived_key union in the
 * HMAC preparekey paths. */
#define REPEAT_BYTE(x) (~0ul / 0xff * (x))
