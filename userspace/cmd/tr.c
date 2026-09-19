/*
 * A cut-down `tr` — enough for a pipeline to have two ends that differ.
 * Plan 9's is `/sys/src/cmd/tr.c` and does rune classes; this does the one
 * thing a pipeline proof needs, and says so.
 */
#include <u.h>
#include <libc.h>

void
main(int argc, char *argv[])
{
	char buf[8192];
	long n, i;

	argv0 = "tr";
	if(argc != 3 || strcmp(argv[1], "a-z") != 0 || strcmp(argv[2], "A-Z") != 0)
		sysfatal("usage: tr a-z A-Z");
	while((n = read(0, buf, sizeof buf)) > 0){
		for(i = 0; i < n; i++)
			if(buf[i] >= 'a' && buf[i] <= 'z')
				buf[i] += 'A' - 'a';
		if(write(1, buf, n) != n)
			sysfatal("write error: %r");
	}
	exits(0);
}
