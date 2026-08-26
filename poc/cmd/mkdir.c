#include "lib9.h"

int
main(int argc, char *argv[])
{
	int i, fd;

	for(i = 1; i < argc; i++){
		fd = create(argv[i], OREAD, DMDIR|0755);
		if(fd < 0){
			fprint(2, "mkdir: %s: %r\n", argv[i]);
			exits("create");
		}
		close(fd);
	}
	return 0;
}
