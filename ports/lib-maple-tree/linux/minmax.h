#pragma once
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
