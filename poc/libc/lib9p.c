#include "lib9.h"
#include "lib9p.h"

uint get16(uchar *p){ return p[0] | p[1]<<8; }
uint get32(uchar *p){ return p[0] | p[1]<<8 | p[2]<<16 | (uint)p[3]<<24; }
uvlong get64(uchar *p){ return (uvlong)get32(p) | (uvlong)get32(p+4)<<32; }
uchar *put8(uchar *p, uint v){ *p++ = v; return p; }
uchar *put16(uchar *p, uint v){ *p++ = v; *p++ = v>>8; return p; }
uchar *put32(uchar *p, uint v){ p = put16(p, v); return put16(p, v>>16); }
uchar *put64(uchar *p, uvlong v){ p = put32(p, (uint)v); return put32(p, (uint)(v>>32)); }
uchar *putstr(uchar *p, char *s){
	int n = strlen(s);
	p = put16(p, n);
	memcpy(p, s, n);
	return p+n;
}
uchar *putqid(uchar *p, int qtype, uvlong qpath){
	p = put8(p, qtype);
	p = put32(p, 0);
	return put64(p, qpath);
}

long
readn(int fd, void *buf, long n)
{
	long got = 0, r;

	while(got < n){
		r = read(fd, (char*)buf+got, n-got);
		if(r <= 0)
			return got ? got : r;
		got += r;
	}
	return n;
}

long
read9msg(int fd, uchar *buf)
{
	long n;

	if(readn(fd, buf, 4) != 4)
		return 0;
	n = get32(buf);
	if(n < 7 || n > MSIZE9)
		return -1;
	if(readn(fd, buf+4, n-4) != n-4)
		return -1;
	return n;
}

void
send9msg(int fd, int type, int tag, uchar *buf, uchar *end)
{
	put32(buf, end - buf);
	put8(buf+4, type);
	put16(buf+5, tag);
	write(fd, buf, end - buf);
}

void
send9err(int fd, int tag, char *e, uchar *buf)
{
	send9msg(fd, Rerror9, tag, buf, putstr(buf+7, e));
}
