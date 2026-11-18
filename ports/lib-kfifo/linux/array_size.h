#pragma once
/* upstream include/linux/array_size.h */
#define ARRAY_SIZE(arr) (sizeof(arr) / sizeof((arr)[0]))
