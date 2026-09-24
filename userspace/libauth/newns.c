/*
 * `newns` — build a namespace from a file.
 *
 * `plan9/sys/src/libauth/newns.c`, with the authentication taken out and
 * nothing else changed: the parse, the argument expansion and the six
 * operations are Plan 9's, line for line.
 *
 * WHAT IS NOT HERE: `factotum` (`/mnt/factotum/rpc`), so `famount` is a
 * `mount` and the `import` operation is gone with `callexport`; `addns`,
 * which is `newns` without the `rfork`; and `atnotify`, which catches an
 * interrupt arriving while the namespace is half built — there are no notes
 * yet, so nothing can arrive.
 *
 * What it is FOR is the whole point of P5: a boot that is configuration
 * rather than code. `init` calls it, it reads `/profile/namespace`, and every
 * line there is a `bind`, a `mount`, a `cd` or a `.`.
 */
#include <u.h>
#include <libc.h>
#include <bio.h>
#include <auth.h>
#include <authsrv.h>	/* ANAMELEN */

enum
{
	NARG	= 15,		/* max number of arguments */
	MAXARG	= 10*ANAMELEN,	/* max length of an argument */
};

static int	setenv(char*, char*);
static char	*expandarg(char*, char*);
static int	splitargs(char*, char*[], char*, int);
static int	nsfile(char*, Biobuf *);
static int	nsop(char*, int, char*[]);

int newnsdebug;

static int
buildns(int newns, char *user, char *file)
{
	Biobuf *b;
	char home[4*ANAMELEN];
	int cdroot;

	if(file == nil){
		if(!newns){
			werrstr("no namespace file specified");
			return -1;
		}
		file = "/profile/namespace";	/* Plan 9: /lib/namespace (docs/packages.md) */
	}
	b = Bopen(file, OREAD);
	if(b == nil){
		werrstr("can't open %s: %r", file);
		return -1;
	}
	if(newns){
		rfork(RFENVG|RFCNAMEG);
		setenv("user", user);
		snprint(home, sizeof home, "/usr/%s", user);
		setenv("home", home);
	}

	cdroot = nsfile(file, b);
	Bterm(b);

	/* make sure we managed to cd into the new name space */
	if(newns && !cdroot && chdir("/") < 0)
		return -1;

	return 0;
}

static int
nsfile(char *fn, Biobuf *b)
{
	int argc;
	char *cmd, *argv[NARG+1], argbuf[MAXARG*NARG];
	int cdroot;

	cdroot = 0;
	while(cmd = Brdline(b, '\n')){
		cmd[Blinelen(b)-1] = '\0';
		while(*cmd==' ' || *cmd=='\t')
			cmd++;
		if(*cmd == '#')
			continue;
		argc = splitargs(cmd, argv, argbuf, NARG);
		if(argc)
			cdroot |= nsop(fn, argc, argv);
	}
	return cdroot;
}

static int
nsop(char *fn, int argc, char *argv[])
{
	char *argv0;
	ulong flags;
	int fd, i;
	Biobuf *b;
	int cdroot;

	cdroot = 0;
	flags = 0;
	argv0 = 0;
	if (newnsdebug){
		for (i = 0; i < argc; i++)
			fprint(2, "%s ", argv[i]);
		fprint(2, "\n");
	}
	ARGBEGIN{
	case 'a':
		flags |= MAFTER;
		break;
	case 'b':
		flags |= MBEFORE;
		break;
	case 'c':
		flags |= MCREATE;
		break;
	case 'C':
		flags |= MCACHE;
		break;
	}ARGEND

	if(!(flags & (MAFTER|MBEFORE)))
		flags |= MREPL;

	if(strcmp(argv0, ".") == 0 && argc == 1){
		b = Bopen(argv[0], OREAD);
		if(b == nil)
			return 0;
		cdroot |= nsfile(fn, b);
		Bterm(b);
	}else if(strcmp(argv0, "clear") == 0 && argc == 0)
		rfork(RFCNAMEG);
	else if(strcmp(argv0, "bind") == 0 && argc == 2){
		if(bind(argv[0], argv[1], flags) < 0 && newnsdebug)
			fprint(2, "%s: bind: %s %s: %r\n", fn, argv[0], argv[1]);
	}else if(strcmp(argv0, "unmount") == 0){
		if(argc == 1)
			unmount(nil, argv[0]);
		else if(argc == 2)
			unmount(argv[0], argv[1]);
	}else if(strcmp(argv0, "mount") == 0){
		fd = open(argv[0], ORDWR);
		if(argc == 2){
			if(mount(fd, -1, argv[1], flags, "") < 0 && newnsdebug)
				fprint(2, "%s: mount: %s %s: %r\n", fn, argv[0], argv[1]);
		}else if(argc == 3){
			if(mount(fd, -1, argv[1], flags, argv[2]) < 0 && newnsdebug)
				fprint(2, "%s: mount: %s %s %s: %r\n", fn, argv[0], argv[1], argv[2]);
		}
		close(fd);
	}else if(strcmp(argv0, "cd") == 0 && argc == 1){
		if(chdir(argv[0]) == 0 && *argv[0] == '/')
			cdroot = 1;
	}
	return cdroot;
}

int
newns(char *user, char *file)
{
	return buildns(1, user, file);
}

int
addns(char *user, char *file)
{
	return buildns(0, user, file);
}

static int
setenv(char *name, char *val)
{
	int f;
	char ename[ANAMELEN+6];

	snprint(ename, sizeof ename, "#e/%s", name);
	f = create(ename, OWRITE, 0664);
	if(f < 0)
		return -1;
	write(f, val, strlen(val));
	close(f);
	return 0;
}

static char*
nextdollar(char *arg)
{
	char *p, *q;

	/* looking for $, skipping over quoted strings */
	for(p = arg; *p; p++){
		if(*p == '\''){
			for(q = p+1; *q; q++)
				if(*q == '\'')
					break;
			if(*q == '\0')
				return nil;
			p = q;
			continue;
		}
		if(*p == '$')
			return p;
	}
	return nil;
}

/*
 * copy a string, expanding any $ variables from /env
 */
static char*
expandarg(char *arg, char *buf)
{
	char env[3+ANAMELEN], *p, *x;
	int fd, n, len;

	n = 0;
	while(p = nextdollar(arg)){
		len = p - arg;
		if(n + len + ANAMELEN >= MAXARG-1)
			return 0;
		memmove(&buf[n], arg, len);
		n += len;
		p++;
		arg = strpbrk(p, "/.!'$");
		if(arg == nil)
			arg = p+strlen(p);
		len = arg - p;
		if(len == 0 || len >= ANAMELEN)
			continue;
		strcpy(env, "#e/");
		strncpy(env+3, p, len);
		env[3+len] = '\0';
		fd = open(env, OREAD);
		if(fd >= 0){
			len = read(fd, &buf[n], ANAMELEN - 1);
			/* some singleton environment variables have trailing NULs */
			/* lists separate entries with NULs; we arbitrarily take the first element */
			if(len > 0){
				x = memchr(&buf[n], 0, len);
				if(x != nil)
					len = x - &buf[n];
				n += len;
			}
			close(fd);
		}
	}
	len = strlen(arg);
	if(n + len >= MAXARG - 1)
		return 0;
	strcpy(&buf[n], arg);
	return &buf[n+len+1];
}

static int
splitargs(char *p, char *argv[], char *argbuf, int nargv)
{
	char *q, *ep, *sp;
	int i, n;

	n = gettokens(p, argv, nargv, " \t");
	if(n == nargv)
		return 0;
	ep = argbuf + MAXARG*nargv;
	for(i = 0; i < n; i++){
		sp = argbuf;
		argbuf = expandarg(argv[i], argbuf);
		if(argbuf == nil || argbuf >= ep)
			break;
		argv[i] = sp;
		/* drop the quotes `newns` leaves in place */
		if(*sp == '\''){
			q = strrchr(sp+1, '\'');
			if(q != nil){
				*q = '\0';
				argv[i] = sp+1;
			}
		}
	}
	argv[i] = nil;
	return i;
}
