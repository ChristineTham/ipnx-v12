/* run: instantiate a process file — M12's local stage (design.md 2026-08-30).
 * A spec is a directory: namespace (namespace(6) subset, applied whole),
 * packages ("name [version]" per line, pkg-verified), env (NAME=value),
 * user (a drop to "none", the only transition there is), cmd (path then
 * one argument per line).
 * A Dockerfile is a script because installing is mutation; this is a
 * declaration because installing is a bind. Order matters and is the
 * doctrine's: detach the namespace, declare the view, land the packages
 * on it, set the environment, drop the credential LAST, exec.
 */
#include "lib9.h"

enum { NARG = 32 };

static char spec[128];

static long
readspec(char *name, char *buf, long max)
{
	char path[192];
	int fd;
	long n;

	snprint(path, sizeof path, "%s/%s", spec, name);
	fd = open(path, OREAD);
	if(fd < 0)
		return -1;
	n = read(fd, buf, max - 1);
	close(fd);
	if(n < 0)
		n = 0;
	buf[n] = 0;
	return n;
}

/* split buf into lines in place; returns count */
static int
lines(char *buf, char **ln, int max)
{
	int n;
	char *p;

	n = 0;
	for(p = buf; *p != 0 && n < max; ){
		ln[n++] = p;
		while(*p != 0 && *p != '\n')
			p++;
		if(*p == '\n')
			*p++ = 0;
	}
	while(n > 0 && ln[n-1][0] == 0)
		n--;
	return n;
}

static void
pkgchild(void *v)
{
	char **av = v;

	exec("/bin/pkg", av);
	fprint(2, "run: exec /bin/pkg: %r\n");
	exits("exec");
}

int
main(int argc, char *argv[])
{
	char nbuf[2048], pbuf[1024], ebuf[1024], ubuf[64], cbuf[1024];
	char *ln[NARG], *av[NARG], *eq, path[192], st[128], *q;
	int i, n, fd;
	long m;

	if(argc != 2){
		fprint(2, "usage: run specdir\n");
		exits("usage");
	}
	snprint(spec, sizeof spec, "%s", argv[1]);

	/* a private view and environment — the process owns its world */
	rfork(RFNAMEG | RFENVG);

	if(readspec("namespace", nbuf, sizeof nbuf) > 0){
		snprint(path, sizeof path, "%s/namespace", spec);
		if(newns(path) < 0){
			fprint(2, "run: newns %s: %r\n", path);
			exits("newns");
		}
	}

	if(readspec("packages", pbuf, sizeof pbuf) > 0){
		n = lines(pbuf, ln, NARG);
		for(i = 0; i < n; i++){
			char *name, *ver, arg[128];
			if(ln[i][0] == 0 || ln[i][0] == '#')
				continue;
			name = ln[i];
			ver = strchr(name, ' ');
			if(ver != nil){
				*ver++ = 0;
				snprint(arg, sizeof arg, "%s@%s", name, ver);
			} else
				snprint(arg, sizeof arg, "%s", name);
			av[0] = "pkg"; av[1] = "install"; av[2] = arg; av[3] = nil;
			procrfork(RFFDG, pkgchild, av);
			m = await(st, sizeof st - 1);
			if(m <= 0)
				exits("await");
			st[m] = 0;
			q = strchr(st, '\'');
			if(q == nil || q[1] != '\''){	/* non-empty exit status */
				fprint(2, "run: pkg install %s failed\n", arg);
				exits("pkg");
			}
		}
	}

	if(readspec("env", ebuf, sizeof ebuf) > 0){
		n = lines(ebuf, ln, NARG);
		for(i = 0; i < n; i++){
			eq = strchr(ln[i], '=');
			if(eq == nil)
				continue;
			*eq++ = 0;
			snprint(path, sizeof path, "/env/%s", ln[i]);
			fd = create(path, OWRITE, 0664);
			if(fd < 0)
				fd = open(path, OWRITE | OTRUNC);
			if(fd >= 0){
				write(fd, eq, strlen(eq));
				close(fd);
			}
		}
	}

	/* the credential drops LAST, so declaring the world never needed more
	 * privilege than the declarer had. A DROP is all this can be: the
	 * kernel's identity is Plan 9's, so the only change a process may make
	 * to itself is becoming "none" (auth.c's userwrite), through /dev/user.
	 * Naming anyone else is refused, and the spec says so. */
	if(readspec("user", ubuf, sizeof ubuf) > 0){
		for(i = 0; ubuf[i] != 0 && ubuf[i] != '\n'; i++)
			;
		ubuf[i] = 0;
		fd = open("/dev/user", OWRITE);
		if(fd < 0 || write(fd, ubuf, strlen(ubuf)) < 0){
			fprint(2, "run: user %s: %r\n", ubuf);
			exits("user");
		}
		close(fd);
	}

	if(readspec("cmd", cbuf, sizeof cbuf) <= 0){
		fprint(2, "run: %s has no cmd\n", spec);
		exits("nocmd");
	}
	n = lines(cbuf, av, NARG - 1);
	if(n == 0)
		exits("nocmd");
	av[n] = nil;
	if(av[0][0] != '/'){
		snprint(path, sizeof path, "/bin/%s", av[0]);
		av[0] = path;
	}
	exec(av[0], av);
	fprint(2, "run: exec %s: %r\n", av[0]);
	exits("exec");
}
