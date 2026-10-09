/*
 * os — run a command on the host (docs/saranos.md, The host's resources).
 *
 * Inferno's os(1), after its source (inferno-os appl/cmd/os.b; man/1/os),
 * without -m mountpoint: where Inferno's opens /cmd/clone and writes to
 * its ctl, this makes one call to the host, which starts the command and
 * has the kernel open its standard input, output and error and its status
 * into descriptors (Christine, 2026-10-09: "9P over #9"). Host commands are
 * explicit: some toolchains depend on the host, and the system says so
 * (Christine, 2026-10-08).
 */
#include <u.h>
#include <libc.h>

/* this machine's call to the host: sys/src/libc/wasm/sys.c */
extern int	oscmd(char**, char**, char*, int, int*);

static void
usage(void)
{
	fprint(2, "usage: os [-d dir] [-n] [-N nice] [-b] command [arg...]\n");
	exits("usage");
}

/*
 * The process's environment, as APE makes one before main
 * (ape/lib/ap/plan9/_envsetup.c:30): each file of #e as name=value, the
 * value's last 0 byte dropped and any other made 1 — a list such as $path
 * holds its words apart with 0s.
 */
static char**
environment(void)
{
	int fd, f, i, j, k;
	long n, m;
	Dir *d;
	char **env, *p, *v, *file;

	fd = open("#e", OREAD);
	if(fd < 0)
		return nil;
	n = dirreadall(fd, &d);
	close(fd);
	if(n < 0)
		return nil;
	env = malloc((n+1)*sizeof(char*));
	if(env == nil)
		sysfatal("malloc: %r");
	j = 0;
	for(i = 0; i < n; i++){
		m = d[i].length;
		p = malloc(strlen(d[i].name)+1+m+1);
		if(p == nil)
			sysfatal("malloc: %r");
		v = p + sprint(p, "%s=", d[i].name);
		file = smprint("#e/%s", d[i].name);
		if(file == nil)
			sysfatal("smprint: %r");
		f = open(file, OREAD);
		if(f < 0 || read(f, v, m) != m)
			m = 0;
		if(f >= 0)
			close(f);
		free(file);
		if(m > 0 && v[m-1] == 0)
			m--;
		for(k = 0; k < m; k++)
			if(v[k] == 0)
				v[k] = 1;
		v[m] = 0;
		env[j++] = p;
	}
	env[j] = nil;
	free(d);
	return env;
}

/* os.b's copy (:141): until either end fails */
static void
pump(int from, int to)
{
	char buf[8192];
	long r;

	for(;;){
		r = read(from, buf, sizeof buf);
		if(r <= 0)
			break;
		if(write(to, buf, r) != r)
			break;
	}
}

/*
 * A process that pumps from to to, as os.b spawns one (:99, :103), holding
 * nothing else of the command's: its end is what ends the command's input,
 * and the main process's going is what kills the command.
 */
static int
copier(int from, int to, int *fd, int wfd)
{
	int pid, i;

	switch(pid = rfork(RFPROC|RFFDG)){
	case -1:
		sysfatal("fork: %r");
	case 0:
		for(i = 0; i < 3; i++)
			if(fd[i] >= 0 && fd[i] != from && fd[i] != to)
				close(fd[i]);
		close(wfd);
		pump(from, to);
		exits(nil);
	}
	return pid;
}

/* os.b's kill (:158) */
static void
kill(int pid)
{
	int fd;
	char buf[32];

	snprint(buf, sizeof buf, "#p/%d/ctl", pid);
	fd = open(buf, OWRITE);
	if(fd < 0)
		return;
	fprint(fd, "kill");
	close(fd);
}

void
main(int argc, char *argv[])
{
	int nice, foreground, fd[3], wfd, pid, epid, w, nf;
	long n;
	char *dir, status[1024], *f[6];

	quotefmtinstall();
	nice = 0;
	dir = nil;
	foreground = 1;
	ARGBEGIN{
	case 'd':
		dir = EARGF(usage());
		break;
	case 'n':
		nice = 1;
		break;
	/* "nice %q" to ctl, and devcmd's atoi (devcmd.c:479) */
	case 'N':
		nice = atoi(EARGF(usage()));
		break;
	case 'b':
		foreground = 0;
		break;
	default:
		usage();
	}ARGEND
	if(argc == 0)
		usage();

	wfd = oscmd(argv, environment(), dir, nice, foreground? fd: nil);
	if(wfd < 0)
		sysfatal("cannot exec: %r");

	if(foreground){
		pid = copier(0, fd[0], fd, wfd);
		close(fd[0]);
		fd[0] = -1;
		epid = copier(fd[2], 2, fd, wfd);
		close(fd[2]);
		fd[2] = -1;
		pump(fd[1], 1);
		close(fd[1]);
		/*
		 * The copier of the command's input is killed, as os.b kills
		 * it (:117). The copier of its error is waited for, where
		 * os.b kills that too (:118): a command's last words of error
		 * can be in hand and not yet copied when its output ends, and
		 * they are what a failed build says.
		 */
		kill(pid);
		while((w = waitpid()) >= 0 && w != epid)
			;
	}

	/* pid user sys real status (os.b:121) */
	n = read(wfd, status, sizeof status - 1);
	if(n < 0)
		sysfatal("wait error: %r");
	status[n] = 0;
	if(n > 0){
		nf = tokenize(status, f, nelem(f));
		if(nf < 5)
			sysfatal("wait error: odd status: %q", status);
		if(f[4][0] != 0)
			exits(smprint("host: %s", f[4]));
	}
	exits(nil);
}
