/* initmouse/initkeyboard: libdraw's platform-IO layer, ours for the same
 * reason libthread.c is — the vendored files leans on kencc's named access
 * to unnamed members, which clang does not carry. Same API (mouse.h,
 * keyboard.h), same wire formats: /dev/mouse 'm' + 4x12, /dev/cons runes,
 * /dev/consctl rawon. The reader procs are libthread threads whose reads
 * park them, not the process. */
#include <u.h>
#include <libc.h>
#include <draw.h>
#include <thread.h>
#include <cursor.h>
#include <mouse.h>
#include <keyboard.h>

static void
mouseproc(void *v)
{
	Mousectl *mc = v;
	char buf[1 + 4*12];
	int n;
	Mouse m;

	for(;;){
		n = read(mc->mfd, buf, sizeof buf);
		if(n <= 0)
			threadexits("mouse eof");
		if(buf[0] == 'r')
			sendul(mc->resizec, 1);
		if(buf[0] != 'm' && buf[0] != 'r')
			continue;
		buf[n < sizeof buf ? n : sizeof buf - 1] = 0;
		m.xy.x = atoi(buf + 1 + 0*12);
		m.xy.y = atoi(buf + 1 + 1*12);
		m.buttons = atoi(buf + 1 + 2*12);
		m.msec = atoi(buf + 1 + 3*12);
		send(mc->c, &m);
	}
}

Mousectl*
initmouse(char *file, Image *i)
{
	Mousectl *mc;
	char buf[64];

	USED(i);
	if(file == nil)
		file = "/dev/mouse";
	mc = mallocz(sizeof(Mousectl), 1);
	mc->mfd = open(file, ORDWR);
	if(mc->mfd < 0){
		free(mc);
		return nil;
	}
	strcpy(buf, file);
	if(strcmp(buf + strlen(buf) - 6, "/mouse") == 0)
		strcpy(buf + strlen(buf) - 6, "/cursor");
	mc->cfd = open(buf, ORDWR);		/* absent is fine */
	mc->c = chancreate(sizeof(Mouse), 0);
	mc->resizec = chancreate(sizeof(int), 2);
	proccreate(mouseproc, mc, 4096);
	return mc;
}

int
readmouse(Mousectl *mc)
{
	Mouse m;

	if(recv(mc->c, &m) < 0)
		return -1;
	mc->xy = m.xy;
	mc->buttons = m.buttons;
	mc->msec = m.msec;
	return 0;
}

void
setcursor(Mousectl *mc, Cursor *c)
{
	if(mc->cfd < 0)
		return;
	if(c == nil)
		write(mc->cfd, "", 0);
	else
		write(mc->cfd, (char*)c, sizeof(Cursor));
}

void
moveto(Mousectl *mc, Point pt)
{
	USED(mc); USED(pt);			/* warping: host policy, v0 ignores */
}

void
closemouse(Mousectl *mc)
{
	if(mc == nil)
		return;
	close(mc->mfd);
	if(mc->cfd >= 0)
		close(mc->cfd);
	free(mc);
}

static void
kbdproc(void *v)
{
	Keyboardctl *kc = v;
	char buf[64], partial[UTFmax + 1];
	Rune r;
	int n, i, np;

	np = 0;
	for(;;){
		n = read(kc->consfd, buf, sizeof buf);
		if(n <= 0)
			threadexits("kbd eof");
		for(i = 0; i < n; i++){
			partial[np++] = buf[i];
			if(fullrune(partial, np)){
				chartorune(&r, partial);
				np = 0;
				send(kc->c, &r);
			}else if(np >= UTFmax)
				np = 0;
		}
	}
}

Keyboardctl*
initkeyboard(char *file)
{
	Keyboardctl *kc;

	if(file == nil)
		file = "/dev/cons";
	kc = mallocz(sizeof(Keyboardctl), 1);
	kc->consfd = open(file, ORDWR);
	if(kc->consfd < 0){
		free(kc);
		return nil;
	}
	kc->ctlfd = open("/dev/consctl", OWRITE);
	if(kc->ctlfd >= 0)
		write(kc->ctlfd, "rawon", 5);
	kc->c = chancreate(sizeof(Rune), 20);
	proccreate(kbdproc, kc, 4096);
	return kc;
}

int
ctlkeyboard(Keyboardctl *kc, char *s)
{
	if(kc->ctlfd < 0)
		return -1;
	return write(kc->ctlfd, s, strlen(s));
}

void
closekeyboard(Keyboardctl *kc)
{
	if(kc == nil)
		return;
	close(kc->consfd);
	if(kc->ctlfd >= 0)
		close(kc->ctlfd);
	free(kc);
}
