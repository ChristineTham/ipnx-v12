/*
 * u.h for the wasm32 architecture.
 *
 * Plan 9 keeps one of these per architecture — `plan9/386/include/u.h`,
 * `arm/include/u.h`, and so on — because the width of a `long`, the shape of a
 * `jmp_buf` and the calling convention behind `va_list` are the machine's
 * business and nothing else's. This is that file for the machine IPNX runs on.
 *
 * It is 386's, with three differences, each forced by the machine:
 *
 *   1. `va_list` is the compiler's. Plan 9's 386 u.h walks the argument frame
 *      by hand (`(char*)(&(start)+1)`), which assumes arguments are in memory
 *      in declaration order. wasm passes them in the engine's value stack, so
 *      only the compiler can know where a variadic argument is; clang supplies
 *      `__builtin_va_*` and there is no portable alternative.
 *   2. `jmp_buf` is four longs, not 386's two: this machine's `setjmp`
 *      cannot save a stack pointer and a pc, and keeps instead which call
 *      it was made in, which `setjmp` it was, and what `longjmp` threw
 *      (`libc/wasm/setjmp.c`). The JMPBUF indices below are 386's and mean
 *      nothing here.
 *   3. The FP control bits are 386's, kept so that a program asking for them
 *      compiles as it does there (`hoc`, `awk`). wasm has no floating-point
 *      control register at all — it rounds to nearest and never traps — and
 *      `libc/wasm/getfcr.c` says so in those terms.
 */
#define nil		((void*)0)
typedef	unsigned short	ushort;
typedef	unsigned char	uchar;
typedef unsigned long	ulong;
typedef unsigned int	uint;
typedef   signed char	schar;
typedef	long long	vlong;
typedef	unsigned long long uvlong;
typedef long		intptr;
typedef unsigned long	uintptr;
typedef unsigned long	usize;
typedef	uint		Rune;
typedef union FPdbleword FPdbleword;
typedef long		jmp_buf[4];
#define	JMPBUFSP	0
#define	JMPBUFPC	1
#define	JMPBUFDPC	0

/* FCR — 386's layout (`plan9/386/include/u.h`) */
#define	FPINEX	(1<<5)
#define	FPUNFL	((1<<4)|(1<<1))
#define	FPOVFL	(1<<3)
#define	FPZDIV	(1<<2)
#define	FPINVAL	(1<<0)
#define	FPRNR	(0<<10)
#define	FPRZ	(3<<10)
#define	FPRPINF	(2<<10)
#define	FPRNINF	(1<<10)
#define	FPRMASK	(3<<10)
#define	FPPEXT	(3<<8)
#define	FPPSGL	(0<<8)
#define	FPPDBL	(2<<8)
#define	FPPMASK	(3<<8)
/* FSR */
#define	FPAINEX	FPINEX
#define	FPAOVFL	FPOVFL
#define	FPAUNFL	FPUNFL
#define	FPAZDIV	FPZDIV
#define	FPAINVAL	FPINVAL
typedef unsigned int	mpdigit;	/* for /sys/include/mp.h */
typedef unsigned char	u8int;
typedef unsigned short	u16int;
typedef unsigned int	u32int;
typedef unsigned long long u64int;

union FPdbleword
{
	double	x;
	struct {	/* little endian */
		ulong lo;
		ulong hi;
	};
};

typedef	__builtin_va_list	va_list;
#define va_start(list, start)	__builtin_va_start(list, start)
#define va_end(list)		__builtin_va_end(list)
#define va_arg(list, mode)	__builtin_va_arg(list, mode)
#define va_copy(d, s)		__builtin_va_copy(d, s)

/*
 * `USED` and `SET` are the Plan 9 compiler's, not a header's: kencc knows both
 * names and uses them to silence its own "set and not used" and "used and not
 * set" diagnostics. Every Plan 9 source assumes them. clang is not that
 * compiler, so the two are defined here — in the machine's header, which is
 * where this build's difference from kencc belongs.
 */
#define USED(...)	_USED(0, __VA_ARGS__)
#define SET(...)	_USED(0, __VA_ARGS__)

/*
 * kencc's `USED` and `SET` take any number of arguments — `<arg.h>`'s ARGEND
 * is `USED(_args, _argt, _argc);` and `unmount.c:21` is `SET(new, old);` — so
 * these do too. They cannot reference them the way kencc's does (they are of
 * no one type), so the build turns off the diagnostics kencc would have used
 * them to silence: see mk.sh.
 */
#define _USED(...)	do{}while(0)
