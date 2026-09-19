/*
 * `ls`, cut to what a system with one directory format needs: the names, or
 * with -l the mode, the length and the name. Plan 9's is `/sys/src/cmd/ls.c`
 * and sorts, columns and follows `-d`; this one does not pretend to be it.
 */
#include <u.h>
#include <libc.h>
#include <fcall.h>	/* dirmodefmt, as ls.c:76 uses it */

static int lflag;

static void
show(Dir *d)
{
	if(lflag)
		print("%M %11lld %s\n", d->mode, d->length, d->name);
	else
		print("%s\n", d->name);
}

static void
ls(char *name)
{
	Dir *d, *all;
	int fd, i, n;

	d = dirstat(name);
	if(d == nil)
		sysfatal("can't stat %s: %r", name);
	if((d->mode & DMDIR) == 0){
		show(d);
		free(d);
		return;
	}
	free(d);
	if((fd = open(name, OREAD)) < 0)
		sysfatal("can't open %s: %r", name);
	while((n = dirread(fd, &all)) > 0){
		for(i = 0; i < n; i++)
			show(&all[i]);
		free(all);
	}
	close(fd);
}

void
main(int argc, char *argv[])
{
	int i;

	argv0 = "ls";
	/* `ls.c:75` installs both: `%M` is a mode and `%q` a quoted name. */
	quotefmtinstall();
	fmtinstall('M', dirmodefmt);
	ARGBEGIN{
	case 'l':
		lflag = 1;
		break;
	}ARGEND
	if(argc == 0)
		ls(".");
	else
		for(i = 0; i < argc; i++)
			ls(argv[i]);
	exits(0);
}
