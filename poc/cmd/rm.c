#include "lib9.h"

int
main(int argc, char *argv[])
{
	int i;

	for(i = 1; i < argc; i++)
		if(remove(argv[i]) < 0){
			fprint(2, "rm: %s: %r\n", argv[i]);
			exits("remove");
		}
	return 0;
}
