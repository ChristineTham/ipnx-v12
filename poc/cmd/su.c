/* su: identity transition under the ctl rules — never escalation.
 * There is no superuser to become and no password to check: the kernel's
 * two rules decide (docs/uid.md). If your ruid is eve you may become
 * anyone (rule 1, both ids move — a full drop); anyone may climb back to
 * their own ruid (rule 2). `su none cmd` is the privilege-drop shell —
 * this system's daily direction. Authenticated transition (a third rule,
 * devcap-shaped) waits on /net and factotum-shaped work.
 *
 * usage: su [user [cmd arg...]]   (no user: climb to your own ruid;
 *                                  no cmd: an interactive rc) */
#include "lib9.h"

int
main(int argc, char *argv[])
{
	char st[128], err[128], *user, *p, *f[8];
	char *rcav[] = { "rc", "-i", nil };
	int fd, n, nf;

	if(argc > 1)
		user = argv[1];
	else{
		/* no user named: the ruid climb, read from our own status */
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
		if(nf < 3){
			fprint(2, "su: can't read own status\n");
			exits("status");
		}
		user = f[2];
	}

	fd = open("/proc/self/ctl", OWRITE);
	if(fd < 0 || fprint(fd, "user %s", user) < 0){
		errstr(err, sizeof err);
		fprint(2, "su: can't become %s: %s\n", user, err);
		exits("denied");
	}
	close(fd);

	if(argc > 2){
		char binpath[128];

		exec(argv[2], argv + 2);        /* as given, then /bin */
		snprint(binpath, sizeof binpath, "/bin/%s", argv[2]);
		exec(binpath, argv + 2);
	}else
		exec("/bin/rc", rcav);
	fprint(2, "su: exec failed\n");
	exits("exec");
}
