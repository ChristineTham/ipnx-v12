/* whoami: the name this process runs under, from /dev/user */
#include "lib9.h"

int
main(int argc, char *argv[])
{
	char buf[64];
	int fd, n;

	USED(argc); USED(argv);

	fd = open("/dev/user", OREAD);
	n = fd >= 0 ? read(fd, buf, sizeof buf - 1) : -1;
	if(n <= 0){
		fprint(2, "whoami: %r\n");
		exits("user");
	}
	buf[n] = 0;
	print("%s\n", buf);
	exits(nil);
	return 0;
}
