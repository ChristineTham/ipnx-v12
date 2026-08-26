/* libv10 stdio: buffered stdout, fprintf/perror over the kernel's errstr. */
#include "lib9.h"

typedef struct FILE FILE;
FILE *stdin = (FILE*)0, *stdout = (FILE*)1, *stderr = (FILE*)2;

static char obuf[512];
static int ocnt;

void
_v10flush(void)
{
	if(ocnt){
		write(1, obuf, ocnt);
		ocnt = 0;
	}
}

int
putchar(int c)
{
	obuf[ocnt++] = c;
	if(c == '\n' || ocnt >= (int)sizeof obuf)
		_v10flush();
	return c;
}

int
fflush(FILE *f)
{
	USED(f);
	_v10flush();
	return 0;
}

int
fprintf(FILE *f, char *fmt, ...)
{
	char buf[512];
	int n;
	__builtin_va_list a;

	__builtin_va_start(a, fmt);
	n = vfmt9(buf, sizeof buf, fmt, a);
	__builtin_va_end(a);
	_v10flush();
	return write(f == (FILE*)1 ? 1 : 2, buf, n);
}

void
perror(char *s)
{
	_v10flush();
	fprint(2, "%s: %r\n", s);
}
