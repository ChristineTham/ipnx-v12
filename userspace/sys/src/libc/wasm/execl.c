/*
 * `execl` — `port/execl.c` is `exec(f, &f+1)`: the arguments after the name
 * are already an array, because on every Plan 9 architecture they sit in
 * memory one after another. On this machine they do not (u.h, `va_list`), so
 * they are gathered into one here, up to the nil that ends them.
 */
#include <u.h>
#include <libc.h>

int
execl(char *f, ...)
{
	va_list arg;
	char **argv;
	int n;

	n = 0;
	va_start(arg, f);
	while(va_arg(arg, char*) != nil)
		n++;
	va_end(arg);
	argv = malloc((n+1)*sizeof(char*));
	if(argv == nil)
		return -1;
	va_start(arg, f);
	for(n = 0; (argv[n] = va_arg(arg, char*)) != nil; n++)
		;
	va_end(arg);
	exec(f, argv);
	free(argv);
	return -1;
}
