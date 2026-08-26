/* ln [-s] target name */
#include "lib9.h"

int
main(int argc, char *argv[])
{
	int sym = 0;

	if(argc == 4 && strcmp(argv[1], "-s") == 0){
		sym = 1;
		argv++;
		argc--;
	}
	if(argc != 3){
		fprint(2, "usage: ln [-s] target name\n");
		exits("usage");
	}
	if((sym ? symlink9(argv[1], argv[2]) : link9(argv[1], argv[2])) < 0){
		fprint(2, "ln: %r\n");
		exits("ln");
	}
	return 0;
}
