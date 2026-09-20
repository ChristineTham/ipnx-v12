/*
 * `mount` — `plan9/sys/src/cmd/mount.c`, with ONE difference.
 *
 * `amount0` there is `fauth` plus `auth_proxy` (p9any against factotum) and
 * then `mount(fd, afd, ...)`. Neither exists here: `fauth` is one of the
 * twelve calls this kernel omits — *"authentication is a file server's,
 * established at attach"* (docs/syscalls.md) — and there is no factotum to
 * proxy to. So it is `mount(fd, -1, ...)`, which is what `-n` asks for
 * anyway, and what this system's own `newns` already does
 * (`userspace/libauth/newns.c`, the same note).
 *
 * Everything else is Plan 9's, including `notify(catch)`: this kernel
 * refuses `notify` and the return value is not checked there either, so the
 * line stays as it is and does nothing.
 */
#include <u.h>
#include <libc.h>

void	usage(void);
void	catch(void*, char*);

char *keyspec = "";

int
amount0(int fd, char *mntpt, int flags, char *aname, char *keyspec)
{
	USED(keyspec);
	return mount(fd, -1, mntpt, flags, aname);
}

void
main(int argc, char *argv[])
{
	char *spec;
	ulong flag = 0;
	int qflag = 0;
	int noauth = 0;
	int fd, rv;

	ARGBEGIN{
	case 'a':
		flag |= MAFTER;
		break;
	case 'b':
		flag |= MBEFORE;
		break;
	case 'c':
		flag |= MCREATE;
		break;
	case 'C':
		flag |= MCACHE;
		break;
	case 'k':
		keyspec = EARGF(usage());
		break;
	case 'n':
		noauth = 1;
		break;
	case 'q':
		qflag = 1;
		break;
	default:
		usage();
	}ARGEND

	spec = 0;
	if(argc == 2)
		spec = "";
	else if(argc == 3)
		spec = argv[2];
	else
		usage();

	if((flag&MAFTER)&&(flag&MBEFORE))
		usage();

	fd = open(argv[0], ORDWR);
	if(fd < 0){
		if(qflag)
			exits(0);
		fprint(2, "%s: can't open %s: %r\n", argv0, argv[0]);
		exits("open");
	}

	notify(catch);
	if(noauth)
		rv = mount(fd, -1, argv[1], flag, spec);
	else
		rv = amount0(fd, argv[1], flag, spec, keyspec);
	if(rv < 0){
		if(qflag)
			exits(0);
		fprint(2, "%s: mount %s: %r\n", argv0, argv[1]);
		exits("mount");
	}
	exits(0);
}

void
catch(void *x, char *m)
{
	USED(x);
	fprint(2, "mount: %s\n", m);
	exits(m);
}

void
usage(void)
{
	fprint(2, "usage: mount [-a|-b] [-cnq] [-k keypattern] /srv/service dir [spec]\n");
	exits("usage");
}
