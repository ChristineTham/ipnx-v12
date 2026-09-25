/*
 * libdynld's machine file for wasm32. Plan 9's is a stub on ten of its
 * fourteen architectures (dynld-amd64.c, dynld-arm64.c, dynld-mips.c, …),
 * answering "unimplemented"; this is the same, and for a reason of its own:
 * a wasm module's code is not in its memory, so object code loaded there
 * cannot be run. a.out.h has no magic number for this machine, so no image
 * is recognised.
 */
#include <u.h>
#include <libc.h>
#include <a.out.h>
#include <dynld.h>

long
dynmagic(void)
{
	return DYN_MAGIC;
}

char*
dynreloc(uchar *b, ulong p, int m, Dynsym **tab, int ntab)
{
	USED(b);
	USED(p);
	USED(m);
	USED(tab);
	USED(ntab);
	return "wasm unimplemented";
}
