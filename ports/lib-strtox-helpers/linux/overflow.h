#pragma once
/* The GCC/Clang builtins give exactly the kernel's wrap+detect semantics. */
#define check_add_overflow(a, b, d)	__builtin_add_overflow(a, b, d)
#define check_mul_overflow(a, b, d)	__builtin_mul_overflow(a, b, d)
