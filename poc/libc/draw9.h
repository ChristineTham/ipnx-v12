/* draw9: a guest libdraw-lite over /dev/draw — enough of draw(3) to paint.
 * Image 0 is the window. Colours are RGBA bytes, premultiplied. */
typedef struct Draw Draw;
struct Draw {
	int	fd;		/* draw/data (the same fd serves new's info) */
	int	w, h;		/* the window image's rectangle */
	int	nextid;
};

int  drawinit(Draw *d);				/* drawattach at /dev */
int  drawattach(Draw *d, char *base);		/* base/draw/new, then base/draw/N/data */
int  drawcolor(Draw *d, int r, int g, int b, int a);	/* 1x1 repl source */
void drawrect(Draw *d, int x0, int y0, int x1, int y1, int src);
void drawline(Draw *d, int x0, int y0, int x1, int y1, int src);
void drawellipse(Draw *d, int cx, int cy, int a, int b, int src, int filled);
void drawfree(Draw *d, int id);
void drawflush(Draw *d);
