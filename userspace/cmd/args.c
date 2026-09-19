#include <u.h>
#include <libc.h>

void
main(int argc, char *argv[])
{
	int i;

	print("argc=%d argv0=%s\n", argc, argv0 ? argv0 : "<nil>");
	for(i = 0; i < argc; i++)
		print("argv[%d]=%s\n", i, argv[i]);
	exits(nil);
}
