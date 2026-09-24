/*
 * `setjmp` and `longjmp` — the machine-dependent pair, as Plan 9's are:
 * `libc/386/setjmp.s` saves SP and the return pc and `longjmp` puts them
 * back. This machine's stack is not addressable, so neither can be saved;
 * what it has instead is wasm's exception handling, and the compiler does the
 * rest (`-mllvm -wasm-enable-sjlj`, mk.sh). A function that calls `setjmp` is
 * compiled to catch `__c_longjmp`, and every call it makes to test, after a
 * catch, whether this jmp_buf was the one thrown to; the three functions
 * below are the half of that the compiler leaves to the library, and their
 * names and shapes are its contract (LLVM's
 * `WebAssemblyLowerEmscriptenEHSjLj.cpp`).
 *
 * `jmp_buf` is four longs here (`u.h`), which is what this needs.
 */
#include <u.h>
#include <libc.h>

typedef struct Jmp Jmp;
struct Jmp
{
	void	*invocation;	/* which call of the function that called setjmp */
	ulong	label;		/* which setjmp in it */
	void	*env;		/* the thrown value: the jmp_buf... */
	int	val;		/* ...and what setjmp is to return */
};

/*
 * The tag `longjmp` throws, which the compiler names and every catch it
 * writes matches: one i32, the address of the Jmp's `env`. It is defined
 * once, here, as the library that throws it; `__builtin_wasm_throw`'s 1
 * below is the compiler's index for it.
 */
__asm__(
	".globl __c_longjmp\n"
	".tagtype __c_longjmp i32\n"
	"__c_longjmp:\n"
);
void
__wasm_setjmp(void *env, ulong label, void *invocation)
{
	Jmp *j;

	j = env;
	j->invocation = invocation;
	j->label = label;
}

ulong
__wasm_setjmp_test(void *env, void *invocation)
{
	Jmp *j;

	j = env;
	if(j->invocation == invocation)
		return j->label;
	return 0;
}

void
__wasm_longjmp(void *env, int val)
{
	Jmp *j;

	j = env;
	/* `setjmp.s`: *"ansi: longjmp(0) => longjmp(1)"* */
	if(val == 0)
		val = 1;
	j->env = env;
	j->val = val;
	__builtin_wasm_throw(1, &j->env);
}
