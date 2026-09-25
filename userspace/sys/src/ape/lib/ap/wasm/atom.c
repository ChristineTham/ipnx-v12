/*
 * `ainc`, `adec` and the compare-and-swap family — `ape/lib/ap/386/atom.s`,
 * with sys9.h's prototypes, as the compiler's builtins, which `libc/wasm/atom.c` says are the same.
 */
#include "../plan9/lib.h"
#include "../plan9/sys9.h"

long
ainc(long *p)
{
	return __atomic_add_fetch(p, 1, __ATOMIC_SEQ_CST);
}

long
adec(long *p)
{
	return __atomic_sub_fetch(p, 1, __ATOMIC_SEQ_CST);
}

int
cas32(unsigned long *p, unsigned long ov, unsigned long nv)
{
	return __atomic_compare_exchange_n(p, &ov, nv, 0, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST);
}

int
cas(unsigned int *p, int ov, int nv)
{
	return cas32((unsigned long*)p, ov, nv);
}

int
casp(void **p, void *ov, void *nv)
{
	return __atomic_compare_exchange_n(p, &ov, nv, 0, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST);
}

int
casl(unsigned long *p, unsigned long ov, unsigned long nv)
{
	return __atomic_compare_exchange_n(p, &ov, nv, 0, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST);
}

int
cas64(unsigned long long *p, unsigned long long ov, unsigned long long nv)
{
	return __atomic_compare_exchange_n(p, &ov, nv, 0, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST);
}
