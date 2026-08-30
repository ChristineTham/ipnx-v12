/* ps: every process, from /proc — pid, effective uid, real uid, parent.
 * Prints nothing where the kernel serves no /proc root listing. */
#include "lib9.h"
#include "lib9p.h"

int
main(int argc, char *argv[])
{
	uchar buf[4096], *p;
	char pid[16], st[128], path[64];
	int fd, sfd, n, m, nl, rl;

	USED(argc); USED(argv);
	fd = open("/proc", OREAD);
	if(fd < 0){
		fprint(2, "ps: /proc: %r\n");
		exits("open");
	}
	while((n = read(fd, buf, sizeof buf)) > 0){
		for(p = buf; p < buf + n; p += rl){
			rl = get16(p) + 2;		/* stat(5) record, self-sized */
			nl = get16(p + 41);
			if(nl > 15) nl = 15;
			memcpy(pid, p + 43, nl);
			pid[nl] = 0;
			snprint(path, sizeof path, "/proc/%s/status", pid);
			sfd = open(path, OREAD);
			if(sfd < 0)
				continue;		/* died between listing and read */
			m = read(sfd, st, sizeof st - 1);
			close(sfd);
			if(m <= 0)
				continue;
			st[m] = 0;
			print("%s", st);		/* pid euid ruid ppid, newline included */
		}
	}
	exits(0);
}
