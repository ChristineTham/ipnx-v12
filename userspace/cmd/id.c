/* id: who am I? euid from /dev/user, ruid from /proc/self/status. */
#include "lib9.h"

int
main(int argc, char *argv[])
{
	char user[64], st[128], *f[8];
	int fd, n, nf;
	char *p;

	USED(argc); USED(argv);
	fd = open("/dev/user", OREAD);
	n = fd >= 0 ? read(fd, user, sizeof user - 1) : 0;
	user[n > 0 ? n : 0] = 0;
	close(fd);
	fd = open("/proc/self/status", OREAD);
	n = fd >= 0 ? read(fd, st, sizeof st - 1) : 0;
	st[n > 0 ? n : 0] = 0;
	close(fd);
	nf = 0;
	f[nf++] = st;
	for(p = st; *p && nf < 8; p++)
		if(*p == ' ' || *p == '\n'){
			*p = 0;
			f[nf++] = p+1;
		}
	print("%s %s\n", user, nf > 2 ? f[2] : "?");
	return 0;
}
