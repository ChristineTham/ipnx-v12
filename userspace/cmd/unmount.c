/* unmount: undo a bind or mount at old (topmost, or the named source). */
#include "lib9.h"

int
main(int argc, char *argv[])
{
	char *name, *old;

	if(argc == 2){ name = nil; old = argv[1]; }
	else if(argc == 3){ name = argv[1]; old = argv[2]; }
	else {
		fprint(2, "usage: unmount [name] old\n");
		exits("usage");
	}
	if(unmount(name, old) < 0){
		fprint(2, "unmount: %s: %r\n", old);
		exits("error");
	}
	exits(0);
}
