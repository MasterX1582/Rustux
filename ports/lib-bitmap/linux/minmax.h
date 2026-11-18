#pragma once
/* include/linux/minmax.h subset — the kernel's min()/max() enforce
 * same-signedness operands; the plain canonical form below preserves the
 * comparison semantics the compiled code actually executes (all call
 * sites here pass two unsigned longs / two unsigned ints). */
#define min(x, y) ({		\
	__typeof__(x) _x = (x);	\
	__typeof__(y) _y = (y);	\
	(void)(&_x == &_y);	\
	_x < _y ? _x : _y; })

#define max(x, y) ({		\
	__typeof__(x) _x = (x);	\
	__typeof__(y) _y = (y);	\
	(void)(&_x == &_y);	\
	_x > _y ? _x : _y; })

#define min_t(type, x, y) ({	\
	type _x = (x);		\
	type _y = (y);		\
	_x < _y ? _x : _y; })

#define max_t(type, x, y) ({	\
	type _x = (x);		\
	type _y = (y);		\
	_x > _y ? _x : _y; })
