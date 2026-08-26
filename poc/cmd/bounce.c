/* bounce: a ball, forever — the animation demo for a draw window. */
#include "lib9.h"
#include "draw9.h"

int
main(int argc, char *argv[])
{
	Draw d;
	int bg, ball, x, y, dx, dy, r;

	USED(argc); USED(argv);
	if(drawinit(&d) < 0){
		fprint(2, "bounce: no /dev/draw: %r\n");
		exits("draw");
	}
	bg = drawcolor(&d, 24, 26, 32, 255);
	ball = drawcolor(&d, 130, 200, 120, 255);
	r = 14;
	x = d.w/3; y = d.h/3; dx = 3; dy = 2;
	for(;;){
		drawrect(&d, 0, 0, d.w, d.h, bg);
		drawellipse(&d, x, y, r, r, ball, 1);
		drawflush(&d);
		x += dx; y += dy;
		if(x-r < 0 || x+r >= d.w) dx = -dx;
		if(y-r < 0 || y+r >= d.h) dy = -dy;
		sleep(16);
	}
	return 0;
}
