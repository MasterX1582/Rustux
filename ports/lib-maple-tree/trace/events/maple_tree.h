#pragma once
/* trace/events/maple_tree.h stub: the real header declares the ma_op /
 * ma_read / ma_write tracepoints; the harness build never emits trace
 * events, so every trace_* call site compiles to nothing and
 * tracepoint_string() passes the function name through unchanged. */
#undef TRACE_SYSTEM
#define TRACE_SYSTEM maple_tree

#define trace_ma_op(func, mas)			do { } while (0)
#define trace_ma_read(func, mas)		do { } while (0)
#define trace_ma_write(func, mas, r_max, entry)	do { } while (0)

#define tracepoint_string(str)			(str)

#define CREATE_TRACE_POINTS
