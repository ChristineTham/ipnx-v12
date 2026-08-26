#include "lib9.h"

int
main(int argc, char *argv[])
{
	int i;

	for(i = 1; i < argc; i++)
		fprint(1, i < argc-1 ? "%s " : "%s", argv[i]);
	write(1, "\n", 1);
	return 0;
}
