#ifndef __STDARG
#define __STDARG

/*
 * The compiler's, as `wasm/include/u.h` says why: 386's walks the argument
 * frame by hand (`386/include/ape/stdarg.h`), and a wasm function's
 * arguments are in the engine's value stack, where only the compiler knows
 * where a variadic one is.
 */
typedef __builtin_va_list va_list;

#define va_start(list, start)	__builtin_va_start(list, start)
#define va_end(list)		__builtin_va_end(list)
#define va_arg(list, mode)	__builtin_va_arg(list, mode)
#define va_copy(d, s)		__builtin_va_copy(d, s)

#endif /* __STDARG */
