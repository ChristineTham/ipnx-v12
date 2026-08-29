/* drtest: the REAL libdraw against #w — geninitdraw speaks /dev/draw/new,
 * getwindow falls back to the display image (the window itself), draw and
 * string go through the real library, and the v0 rgb file checks pixels. */
#include <u.h>
#include <libc.h>
#include <draw.h>

static int npass, nfail;
static void
ok(int cond, char *what)
{
	if(cond){ npass++; print("PASS %s\n", what); }
	else    { nfail++; print("FAIL %s\n", what); }
}

static int rgbfd;
static ulong
pixel(int x, int y)
{
	uchar px[4];

	if(pread(rgbfd, px, 4, (vlong)(y*400 + x)*4) != 4)
		return 0xDEAD;
	return (ulong)px[0]<<24 | px[1]<<16 | px[2]<<8 | px[3];
}

void
main(int argc, char *argv[])
{
	char buf[16], path[32];
	int fd, n, x, y, inked;
	Image *red;

	USED(argc); USED(argv);
	fd = open("#w/clone", OREAD);
	n = fd >= 0 ? read(fd, buf, 15) : -1;
	buf[n > 0 ? n : 0] = 0;
	strcpy(path, "#w/");
	strcpy(path+3, buf);
	bind(path, "/n/win", MREPL);

	if(geninitdraw("/n/win", nil, nil, "drtest", "/n/win", Refnone) < 0)
		sysfatal("initdraw: %r");
	ok(display != nil && screen != nil, "real libdraw: geninitdraw over a #w window");
	ok(Dx(screen->r) == 400 && Dy(screen->r) == 300,
	   "getwindow fallback: the screen IS the window, 400x300");

	draw(screen, Rect(10,10,110,60), display->black, nil, ZP);
	red = allocimage(display, Rect(0,0,1,1), RGBA32, 1, DRed);
	ok(red != nil, "allocimage: a 1x1 repl colour");
	draw(screen, Rect(120,10,220,60), red, nil, ZP);
	string(screen, Pt(10,80), display->black, ZP, font, "hello from the real libdraw");
	flushimage(display, 1);

	rgbfd = open("/n/win/rgb", OREAD);
	ok(pixel(50,30) == 0x000000FF, "draw: the black rectangle landed");
	ok(pixel(170,30) == 0xFF0000FF, "draw: DRed through the real allocimage");
	ok(pixel(300,200) == 0xFFFFFFFF, "the background is untouched white");
	inked = 0;
	for(y = 68; y < 92 && !inked; y++)
		for(x = 10; x < 250; x++)
			if(pixel(x, y) == 0x000000FF){ inked = 1; break; }
	ok(inked, "string: the default font (defont, via y/i/l/s) put ink down");

	if(nfail == 0)
		print("drtest: all %d passed\n", npass);
	exits(nfail ? "fail" : "");
}
