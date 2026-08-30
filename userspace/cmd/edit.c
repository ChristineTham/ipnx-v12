/* edit: acme-today's first slice — the one editor, as a canvas policy
 * client (M5, docs/userland.md). A window is a tag row and a body: the
 * tag holds the file name (action=look: re-read from disk) and two
 * honest buttons, Put (action=execute: write the body back) and Del;
 * the body is one edit node. Layout, wrapping, selection, IME — the
 * presenter's. The sam Edit language is the recorded next step and
 * rides the addr file when it returns.
 *
 * Single-threaded: unlike con(1) there is no second stream to watch —
 * one blocking read on events is the whole loop. The body is shadowed
 * from its events (never re-read), con's discipline.
 */
#include "lib9.h"

int tokenize(char*, char**, int);	/* libp9's, linked in every cmd */

enum { BMAX = 65536 };

static char body[BMAX];
static long blen;
static char wdir[64];
static int ctlfd, addrfd, datafd;

static long
readfile(char *path, char *buf, long max)
{
	int fd;
	long n, got;

	fd = open(path, OREAD);
	if(fd < 0)
		return 0;			/* a new file starts empty */
	got = 0;
	while(got < max - 1 && (n = read(fd, buf + got, max - 1 - got)) > 0)
		got += n;
	close(fd);
	buf[got] = 0;
	return got;
}

static void
putbody(char *path)
{
	int fd;

	fd = create(path, OWRITE, 0644);
	if(fd < 0){
		fprint(2, "edit: cannot write %s: %r\n", path);
		return;
	}
	write(fd, body, blen);
	close(fd);
}

static void
setbody(void)
{
	fprint(addrfd, "0,$");
	write(datafd, body, blen);
	fprint(ctlfd, "sync");
}

static int
unq(char *s, char *out, int max)
{
	int n;

	n = 0;
	while(*s != 0 && *s != '\n' && n < max - 1){
		if(s[0] == '%' && s[1] != 0 && s[2] != 0){
			if(s[1] == '2' && s[2] == '0'){ out[n++] = ' '; s += 3; continue; }
			if(s[1] == '0' && (s[2] == 'A' || s[2] == 'a')){ out[n++] = '\n'; s += 3; continue; }
			if(s[1] == '2' && s[2] == '5'){ out[n++] = '%'; s += 3; continue; }
		}
		out[n++] = *s++;
	}
	out[n] = 0;
	return n;
}

int
main(int argc, char *argv[])
{
	char path[128], line[4096], txt[4096], *f[8], *file;
	int wid, evfd, fd, nf, i;
	long n, q0, q1, d;

	wid = -1;
	i = 1;
	if(argc > 2 && strcmp(argv[1], "-W") == 0){
		wid = atoi(argv[2]);
		i = 3;
	}
	if(argc - i != 1){
		fprint(2, "usage: edit [-W wid] file\n");
		exits("usage");
	}
	file = argv[i];

	if(wid < 0){
		fd = open("#w/clone", OREAD);
		if(fd < 0)
			sysfatal("no window server: %r");
		n = read(fd, path, 15);
		close(fd);
		if(n <= 0)
			sysfatal("clone read failed");
		path[n] = 0;
		wid = atoi(path);
	}
	snprint(wdir, sizeof wdir, "#w/%d", wid);

	snprint(path, sizeof path, "%s/canvas/ctl", wdir);
	ctlfd = open(path, OWRITE);
	if(ctlfd < 0)
		sysfatal("no canvas on this host: %r");

	/* the tree: a column of tag row and body.
	 * 1 = tag (stack row), 2 = file name (look), 3 = Put, 4 = Del,
	 * 5 = the body (edit). Structure in attrs, acme's shape. */
	fprint(ctlfd, "new 1 stack\nnew 2 text\nnew 3 text\nnew 4 text\nnew 5 edit\n");
	for(i = 2; i <= 4; i++){
		snprint(path, sizeof path, "%s/canvas/%d/attrs", wdir, i);
		fd = open(path, OWRITE);
		if(fd >= 0){
			fprint(fd, "parent=1\norder=%d\n%s", i,
				i == 2 ? "action=look\n" : "action=execute\n");
			close(fd);
		}
	}
	snprint(path, sizeof path, "%s/canvas/1/attrs", wdir);
	fd = open(path, OWRITE);
	if(fd >= 0){ fprint(fd, "dir=row\norder=1\n"); close(fd); }
	snprint(path, sizeof path, "%s/canvas/5/attrs", wdir);
	fd = open(path, OWRITE);
	if(fd >= 0){ fprint(fd, "order=2\n"); close(fd); }

	for(i = 2; i <= 4; i++){
		char *t = i == 2 ? file : i == 3 ? "Put" : "Del";
		snprint(path, sizeof path, "%s/canvas/%d/data", wdir, i);
		fd = open(path, OWRITE);
		if(fd >= 0){ write(fd, t, strlen(t)); close(fd); }
	}

	snprint(path, sizeof path, "%s/canvas/5/addr", wdir);
	addrfd = open(path, OWRITE);
	snprint(path, sizeof path, "%s/canvas/5/data", wdir);
	datafd = open(path, OWRITE);
	snprint(path, sizeof path, "%s/canvas/events", wdir);
	evfd = open(path, OREAD);
	if(addrfd < 0 || datafd < 0 || evfd < 0)
		sysfatal("canvas files: %r");

	snprint(path, sizeof path, "%s/label", wdir);
	fd = open(path, OWRITE);
	if(fd >= 0){ fprint(fd, "edit %s", file); close(fd); }

	blen = readfile(file, body, sizeof body);
	setbody();

	for(;;){
		n = read(evfd, line, sizeof line - 1);	/* one event per read */
		if(n <= 0)
			break;
		line[n] = 0;
		nf = tokenize(line, f, 8);
		if(nf >= 4 && strcmp(f[0], "insert") == 0 && atoi(f[1]) == 5){
			q0 = atol(f[2]);
			d = unq(f[3], txt, sizeof txt);
			if(q0 > blen) q0 = blen;
			if(blen + d > BMAX - 1) d = BMAX - 1 - blen;
			memmove(body + q0 + d, body + q0, blen - q0);
			memmove(body + q0, txt, d);
			blen += d;
			continue;
		}
		if(nf >= 4 && strcmp(f[0], "delete") == 0 && atoi(f[1]) == 5){
			q0 = atol(f[2]);
			q1 = atol(f[3]);
			if(q1 > blen) q1 = blen;
			if(q0 > q1) q0 = q1;
			memmove(body + q0, body + q1, blen - q1);
			blen -= q1 - q0;
			continue;
		}
		if(nf >= 2 && strcmp(f[0], "execute") == 0 && atoi(f[1]) == 3){
			putbody(file);			/* Put */
			continue;
		}
		if(nf >= 2 && strcmp(f[0], "look") == 0 && atoi(f[1]) == 2){
			blen = readfile(file, body, sizeof body);	/* Get */
			setbody();
			continue;
		}
		if((nf >= 2 && strcmp(f[0], "execute") == 0 && atoi(f[1]) == 4)
		|| strcmp(f[0], "close") == 0)
			break;				/* Del, or the surface asked */
	}
	snprint(path, sizeof path, "%s/wctl", wdir);
	fd = open(path, OWRITE);
	if(fd >= 0){ fprint(fd, "delete"); close(fd); }
	exits(0);
}
