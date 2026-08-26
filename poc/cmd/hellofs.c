/* hellofs: a 9P2000 file server in a guest process, serving on fd 0.
 * The other end of its pipe gets mount(2)ed — this is the wire boundary
 * the kernel's Dev table stops at (docs/syscalls.md), and the proof that
 * a user process can be a filesystem.
 *
 * Tree: /  motd (read-only text)  note (writable, 256 bytes).
 * Unsupported requests get Rerror, which the client sees as errstr.
 */
#include "lib9.h"

enum {
	Tversion = 100, Tauth = 102, Tattach = 104, Twalk = 110,
	Topen = 112, Tcreate = 114, Tread = 116, Twrite = 118,
	Tclunk = 120, Tremove = 122, Tstat = 124, Twstat = 126,
	Rerr = 107,
	MSIZE = 8216,
	QTDIR = 0x80, QTFILE = 0,
	NFID = 32,
};

static char motd[] = "served over wire 9P by a wasm process\n";
static char note[256];
static int notelen;

/* nodes: 0 root, 1 motd, 2 note */
static int fids[NFID], fidnode[NFID];

static uchar msg[MSIZE], out[MSIZE];

/* ---- little-endian get/put ---- */
static uint g16(uchar *p){ return p[0] | p[1]<<8; }
static uint g32(uchar *p){ return p[0] | p[1]<<8 | p[2]<<16 | p[3]<<24; }
static uvlong g64(uchar *p){ return (uvlong)g32(p) | (uvlong)g32(p+4)<<32; }
static uchar *p8(uchar *p, uint v){ *p++ = v; return p; }
static uchar *p16(uchar *p, uint v){ *p++ = v; *p++ = v>>8; return p; }
static uchar *p32(uchar *p, uint v){ p = p16(p, v); return p16(p, v>>16); }
static uchar *p64(uchar *p, uvlong v){ p = p32(p, (uint)v); return p32(p, (uint)(v>>32)); }
static uchar *pstr(uchar *p, char *s){ int n = strlen(s); p = p16(p, n); memcpy(p, s, n); return p+n; }
static uchar *pqid(uchar *p, int node){
	p = p8(p, node == 0 ? QTDIR : QTFILE);
	p = p32(p, 0);
	return p64(p, node+100);
}

static uchar *
pstat(uchar *p, int node)
{
	static char *names[] = { "/", "motd", "note" };
	uchar *sz = p;

	p = p16(p, 0);				/* record size, patched below */
	p = p16(p, 0); p = p32(p, 0);		/* type, dev */
	p = pqid(p, node);
	p = p32(p, node == 0 ? 0x80000000|0755 : node == 2 ? 0666 : 0444);
	p = p32(p, 0); p = p32(p, 0);		/* atime, mtime */
	p = p64(p, node == 1 ? sizeof motd - 1 : node == 2 ? notelen : 0);
	p = pstr(p, names[node]);
	p = pstr(p, "hellofs"); p = pstr(p, "hellofs"); p = pstr(p, "hellofs");
	p16(sz, p - sz - 2);
	return p;
}

static int
findfid(int fid, int alloc)
{
	int i, free_ = -1;

	for(i = 0; i < NFID; i++){
		if(fids[i] == fid)
			return i;
		if(fids[i] == -1 && free_ < 0)
			free_ = i;
	}
	if(alloc && free_ >= 0){
		fids[free_] = fid;
		return free_;
	}
	return -1;
}

static long
readn(int fd, uchar *buf, long n)
{
	long got = 0, r;

	while(got < n){
		r = read(fd, buf+got, n-got);
		if(r <= 0)
			return got ? got : r;
		got += r;
	}
	return n;
}

static void
sendmsg(int type, int tag, uchar *end)
{
	p32(out, end - out);
	p8(out+4, type);
	p16(out+5, tag);
	write(1, out, end - out);
}

static void
rerror(int tag, char *e)
{
	uchar *p = pstr(out+7, e);
	sendmsg(Rerr, tag, p);
}

int
main(int argc, char *argv[])
{
	int i, type, tag, fid, nf, node;
	uchar *p, *b;
	uint n, count;
	uvlong off;

	USED(argc); USED(argv);
	dup(0, 1);				/* serve both ways on the pipe */
	for(i = 0; i < NFID; i++)
		fids[i] = -1;
	for(;;){
		if(readn(0, msg, 4) != 4)
			exits(nil);		/* client side gone */
		n = g32(msg);
		if(n < 7 || n > sizeof msg){
			exits("bad message size");
		}
		if(readn(0, msg+4, n-4) != n-4)
			exits("short message");
		type = msg[4];
		tag = g16(msg+5);
		b = msg+7;
		p = out+7;
		switch(type){
		case Tversion:
			count = g32(b);
			p = p32(p, count < MSIZE ? count : MSIZE);
			p = pstr(p, "9P2000");
			sendmsg(type+1, tag, p);
			break;
		case Tattach:
			fid = g32(b);
			nf = findfid(fid, 1);
			if(nf < 0){ rerror(tag, "out of fids"); break; }
			fidnode[nf] = 0;
			p = pqid(p, 0);
			sendmsg(type+1, tag, p);
			break;
		case Twalk: {
			int newfid = g32(b+4), nname = g16(b+8);
			char name[64];
			uint ln;

			nf = findfid(fid = g32(b), 0);
			if(nf < 0){ rerror(tag, "unknown fid"); break; }
			node = fidnode[nf];
			if(nname == 0){
				nf = findfid(newfid, 1);
				fidnode[nf] = node;
				p = p16(p, 0);
				sendmsg(type+1, tag, p);
				break;
			}
			if(nname != 1){ rerror(tag, "one name per walk here"); break; }
			ln = g16(b+10);
			if(ln > 63){ rerror(tag, "name too long"); break; }
			memcpy(name, b+12, ln);
			name[ln] = 0;
			if(node != 0){ rerror(tag, "walk in non-directory"); break; }
			if(strcmp(name, "motd") == 0) node = 1;
			else if(strcmp(name, "note") == 0) node = 2;
			else { rerror(tag, "file not found"); break; }
			nf = findfid(newfid, 1);
			if(nf < 0){ rerror(tag, "out of fids"); break; }
			fidnode[nf] = node;
			p = p16(p, 1);
			p = pqid(p, node);
			sendmsg(type+1, tag, p);
			break;
		}
		case Topen:
			nf = findfid(g32(b), 0);
			if(nf < 0){ rerror(tag, "unknown fid"); break; }
			p = pqid(p, fidnode[nf]);
			p = p32(p, 0);		/* iounit */
			sendmsg(type+1, tag, p);
			break;
		case Tread: {
			uchar dir[512], *d, *e;
			nf = findfid(g32(b), 0);
			if(nf < 0){ rerror(tag, "unknown fid"); break; }
			node = fidnode[nf];
			off = g64(b+4);
			count = g32(b+12);
			if(count > MSIZE-24) count = MSIZE-24;
			if(node == 0){
				/* whole listing, then an integral slice, per read(5) */
				d = pstat(dir, 1);
				e = pstat(d, 2);
				n = 0;
				d = dir;
				while(d < e && (uvlong)(d-dir) < off)
					d += g16(d)+2;
				while(d+ (g16(d)+2) <= e && n + g16(d)+2 <= count && d < e){
					memcpy(p+4+n, d, g16(d)+2);
					n += g16(d)+2;
					d += g16(d)+2;
					if(d >= e) break;
				}
				p32(p, n);
				p += 4+n;
			} else {
				char *src = node == 1 ? motd : note;
				uint len = node == 1 ? sizeof motd - 1 : (uint)notelen;
				if(off > len) off = len;
				n = len - (uint)off;
				if(n > count) n = count;
				p32(p, n);
				memcpy(p+4, src+off, n);
				p += 4+n;
			}
			sendmsg(type+1, tag, p);
			break;
		}
		case Twrite:
			nf = findfid(g32(b), 0);
			if(nf < 0){ rerror(tag, "unknown fid"); break; }
			if(fidnode[nf] != 2){ rerror(tag, "read-only file"); break; }
			off = g64(b+4);
			count = g32(b+12);
			if(off + count > sizeof note){ rerror(tag, "note is small"); break; }
			memcpy(note+off, b+16, count);
			if((int)(off+count) > notelen)
				notelen = off+count;
			p = p32(p, count);
			sendmsg(type+1, tag, p);
			break;
		case Tclunk:
		case Tremove:			/* remove also clunks; we refuse the removal part */
			nf = findfid(g32(b), 0);
			if(nf >= 0)
				fids[nf] = -1;
			if(type == Tremove){ rerror(tag, "not removable"); break; }
			sendmsg(type+1, tag, p);
			break;
		case Tstat: {
			uchar *sz;
			nf = findfid(g32(b), 0);
			if(nf < 0){ rerror(tag, "unknown fid"); break; }
			sz = p;			/* stat(5): the record, twice counted */
			p = p16(p, 0);
			p = pstat(p, fidnode[nf]);
			p16(sz, p - sz - 2);
			sendmsg(type+1, tag, p);
			break;
		}
		default:
			rerror(tag, "hellofs: unsupported request");
		}
	}
	return 0;
}
