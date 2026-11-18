#pragma once
/* Kernel <linux/ctype.h> semantics: only ASCII 'A'-'Z' fold.  __tolower()
 * takes unsigned char, so negative chars (bytes >= 0x80) wrap to their
 * byte value and pass through unchanged, exactly like the _ctype[] table. */
static inline unsigned char __tolower(unsigned char c)
{
	if (c >= 'A' && c <= 'Z')
		c -= 'A' - 'a';
	return c;
}

static inline unsigned char __toupper(unsigned char c)
{
	if (c >= 'a' && c <= 'z')
		c -= 'a' - 'A';
	return c;
}

#define tolower(c) __tolower(c)
#define toupper(c) __toupper(c)
