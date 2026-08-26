#include "lib9.h"
#include "lib9p.h"	/* put8/put16/put32: low-order byte first, as draw(3) wants */
#include "draw9.h"

static uchar buf[128];

static long
atoin(char *p, int n)
{
	long v = 0;
	int i;

	for(i = 0; i < n; i++)
		if(p[i] >= '0' && p[i] <= '9')
			v = v*10 + p[i]-'0';
	return v;
}

int
drawattach(Draw *d, char *base)
{
	char info[160], path[64];
	long n, conn;
	int newfd, i;

	strcpy(path, base);
	strcpy(path+strlen(path), "/draw/new");
	newfd = open(path, ORDWR);
	if(newfd < 0)
		return -1;
	n = read(newfd, info, sizeof info - 1);
	close(newfd);
	if(n < 12*12)
		return -1;
	/* 12 fields, 12 chars each: conn dispid chan repl r(4) clip(4) */
	conn = atoin(info, 12);
	d->w = atoin(info + 6*12, 12);
	d->h = atoin(info + 7*12, 12);
	strcpy(path, base);
	i = strlen(path);
	strcpy(path+i, "/draw/");
	i += 6;
	if(conn >= 10){ path[i++] = '0' + conn/10; }
	path[i++] = '0' + conn%10;
	strcpy(path+i, "/data");
	d->fd = open(path, OWRITE);
	if(d->fd < 0)
		return -1;
	d->nextid = 1;
	return 0;
}

int
drawinit(Draw *d)
{
	return drawattach(d, "/dev");
}

int
drawcolor(Draw *d, int r, int g, int b, int a)
{
	uchar *p = buf;
	int id = d->nextid++;

	p = put8(p, 'b');
	p = put32(p, id);
	p = put32(p, 0);		/* screenid */
	p = put8(p, 0);			/* refresh */
	p = put32(p, 0);		/* chan (r8g8b8a8 implied, v0) */
	p = put8(p, 1);			/* repl */
	p = put32(p, 0); p = put32(p, 0); p = put32(p, 1); p = put32(p, 1);
	p = put32(p, 0); p = put32(p, 0); p = put32(p, 1); p = put32(p, 1);
	p = put8(p, r); p = put8(p, g); p = put8(p, b); p = put8(p, a);
	write(d->fd, buf, p - buf);
	return id;
}

void
drawrect(Draw *d, int x0, int y0, int x1, int y1, int src)
{
	uchar *p = buf;

	p = put8(p, 'd');
	p = put32(p, 0);		/* dst: the window */
	p = put32(p, src);
	p = put32(p, 0);		/* mask: opaque (v0) */
	p = put32(p, x0); p = put32(p, y0); p = put32(p, x1); p = put32(p, y1);
	p = put32(p, 0); p = put32(p, 0);	/* srcpt */
	p = put32(p, 0); p = put32(p, 0);	/* maskpt */
	write(d->fd, buf, p - buf);
}

void
drawline(Draw *d, int x0, int y0, int x1, int y1, int src)
{
	uchar *p = buf;

	p = put8(p, 'L');
	p = put32(p, 0);
	p = put32(p, x0); p = put32(p, y0);
	p = put32(p, x1); p = put32(p, y1);
	p = put32(p, 0); p = put32(p, 0);	/* endcaps */
	p = put32(p, 0);			/* thickness */
	p = put32(p, src);
	p = put32(p, 0); p = put32(p, 0);	/* sp */
	write(d->fd, buf, p - buf);
}

void
drawellipse(Draw *d, int cx, int cy, int a, int b, int src, int filled)
{
	uchar *p = buf;

	p = put8(p, filled ? 'E' : 'e');
	p = put32(p, 0);
	p = put32(p, cx); p = put32(p, cy);
	p = put32(p, a); p = put32(p, b);
	p = put32(p, 0);			/* thickness */
	p = put32(p, src);
	p = put32(p, 0); p = put32(p, 0);
	write(d->fd, buf, p - buf);
}

void
drawfree(Draw *d, int id)
{
	uchar *p = buf;

	p = put8(p, 'f');
	p = put32(p, id);
	write(d->fd, buf, p - buf);
}

void
drawflush(Draw *d)
{
	uchar c = 'v';

	write(d->fd, &c, 1);
}
