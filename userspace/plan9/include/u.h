/* shim u.h — the platform layer beneath the REAL /sys/include/libc.h
 * (which is vendored, verbatim, at ../sys/include/libc.h). Exactly the
 * split Plan 9 itself uses: u.h is per-platform, libc.h is the system.
 * This platform is wasm32 under clang. Rune is 32-bit: the vendored
 * snapshot is the LATE 4th edition (libc.h: Runemax = 0x10FFFF, UTFmax = 4;
 * rune.c does surrogates), and one place needs Runemax to round-trip
 * through a Rune — sam's class-range sentinel (regexp.c: classp[n] =
 * Runemax) — which a 16-bit Rune truncates, silently killing every [a-z]
 * range (measured 2026-08-29, RESEARCH §9.4). */
typedef unsigned char	uchar;
typedef signed char	schar;
typedef unsigned short	ushort;
typedef unsigned int	uint;
typedef unsigned long	ulong;
typedef long long	vlong;
typedef unsigned long long uvlong;
typedef unsigned char	u8int;
typedef unsigned short	u16int;
typedef unsigned int	u32int;
typedef unsigned long long u64int;
typedef unsigned int	uintptr;
typedef unsigned int	Rune;
typedef ulong		mpdigit;	/* referenced by libc.h decls; unused */
typedef char		s8int;
typedef short		s16int;
typedef int		s32int;
typedef long long	s64int;

typedef __builtin_va_list va_list;
#define va_start(v,l)	__builtin_va_start(v,l)
#define va_end(v)	__builtin_va_end(v)
#define va_arg(v,t)	__builtin_va_arg(v,t)
#define va_copy(v,w)	__builtin_va_copy(v,w)

#define nil		((void*)0)
#define USED(...)
#define SET(x)		((x) = 0)

typedef union FPdbleword FPdbleword;	/* wasm is little-endian, like 386 */
union FPdbleword
{
	double	x;
	struct {	/* little endian */
		ulong	lo;
		ulong	hi;
	};
};
typedef long jmp_buf[10];		/* declared in libc.h; no setjmp here */
