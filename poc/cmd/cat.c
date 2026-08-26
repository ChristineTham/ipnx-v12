#include "lib9.h"

static void
copy(int fd, char *name)
{
	char buf[8192];
	long n;

	while((n = read(fd, buf, sizeof buf)) > 0)
		if(write(1, buf, n) != n)
			exits("write");
	if(n < 0)
		fprint(2, "cat: %s: %r\n", name);
}

int
main(int argc, char *argv[])
{
	int i, fd;

	if(argc == 1)
		copy(0, "<stdin>");
	for(i = 1; i < argc; i++){
		fd = open(argv[i], OREAD);
		if(fd < 0){
			fprint(2, "cat: can't open %s: %r\n", argv[i]);
			exits("open");
		}
		copy(fd, argv[i]);
		close(fd);
	}
	return 0;
}
