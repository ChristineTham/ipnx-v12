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
	p = put32(p, 0x08182848);	/* chan: r8g8b8a8 */
	p = put8(p, 1);			/* repl */
	p = put32(p, 0); p = put32(p, 0); p = put32(p, 1); p = put32(p, 1);
	p = put32(p, 0); p = put32(p, 0); p = put32(p, 1); p = put32(p, 1);
	/* colour is RGBA32, low-order byte first on the wire: a b g r */
	p = put8(p, a); p = put8(p, b); p = put8(p, g); p = put8(p, r);
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

/* ---- text: upload font8x8 as a 96x8-cell cache, draw strings with s ---- */
#include "font8x8.h"

enum { GW = 8, GH = 8, NGLYPH = 95 };

int
drawfontinit(Draw *d)
{
	static uchar big[64 + 21 + GW*GH*4];
	uchar *p;
	int id, ch, row, col, i;

	id = d->nextid++;
	p = big;
	p = put8(p, 'b');			/* the cache image: NGLYPH*GW wide */
	p = put32(p, id);
	p = put32(p, 0);
	p = put8(p, 0);
	p = put32(p, 0x08182848);	/* chan: r8g8b8a8 */
	p = put8(p, 0);
	p = put32(p, 0); p = put32(p, 0); p = put32(p, NGLYPH*GW); p = put32(p, GH);
	p = put32(p, 0); p = put32(p, 0); p = put32(p, NGLYPH*GW); p = put32(p, GH);
	p = put8(p, 0); p = put8(p, 0); p = put8(p, 0); p = put8(p, 0);
	write(d->fd, big, p - big);
	p = big;				/* declare it a font */
	p = put8(p, 'i');
	p = put32(p, id);
	p = put32(p, NGLYPH);
	p = put8(p, 7);				/* ascent */
	write(d->fd, big, p - big);
	for(ch = 0; ch < NGLYPH; ch++){
		p = big;			/* y: this glyph's pixels */
		p = put8(p, 'y');
		p = put32(p, id);
		p = put32(p, ch*GW); p = put32(p, 0);
		p = put32(p, ch*GW+GW); p = put32(p, GH);
		for(row = 0; row < GH; row++)
			for(col = 0; col < GW; col++){
				int on = font8x8[ch][row] & (0x80 >> col);
				for(i = 0; i < 4; i++)
					*p++ = on ? 0xFF : 0x00;
			}
		write(d->fd, big, p - big);
		p = big;			/* l: record the slot (self-copy) */
		p = put8(p, 'l');
		p = put32(p, id);
		p = put32(p, id);
		p = put16(p, ch);
		p = put32(p, ch*GW); p = put32(p, 0);
		p = put32(p, ch*GW+GW); p = put32(p, GH);
		p = put32(p, ch*GW); p = put32(p, 0);
		p = put8(p, 0);			/* left */
		p = put8(p, GW);		/* width */
		write(d->fd, big, p - big);
	}
	return id;
}

void
drawtext(Draw *d, int x, int y, char *s, int fontid, int src)
{
	static uchar buf[64 + 2*256];
	uchar *p;
	int n;

	n = strlen(s);
	if(n > 256)
		n = 256;
	p = buf;
	p = put8(p, 's');
	p = put32(p, 0);			/* dst: the window */
	p = put32(p, src);
	p = put32(p, fontid);
	p = put32(p, x); p = put32(p, y);
	p = put32(p, 0); p = put32(p, 0); p = put32(p, d->w); p = put32(p, d->h);
	p = put32(p, 0); p = put32(p, 0);	/* sp */
	p = put16(p, n);
	while(n--){
		int c = *s++ - ' ';
		p = put16(p, c < 0 || c >= NGLYPH ? 0 : c);
	}
	write(d->fd, buf, p - buf);
}
