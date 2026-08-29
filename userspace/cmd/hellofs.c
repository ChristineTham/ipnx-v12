/* hellofs: a 9P2000 file server in a guest process, serving on fd 0.
 * The other end of its pipe gets mount(2)ed — this is the wire boundary
 * the kernel's Dev table stops at (docs/syscalls.md), and the proof that
 * a user process can be a filesystem.
 *
 * Tree: /  motd (read-only text)  note (writable, 256 bytes).
 * Unsupported requests get Rerror, which the client sees as errstr.
 */
#include "lib9.h"
#include "lib9p.h"

enum { NFID = 32 };

static char motd[] = "served over wire 9P by a wasm process\n";
static char note[256];
static int notelen;

/* nodes: 0 root, 1 motd, 2 note */
static int fids[NFID], fidnode[NFID];

static uchar msg[MSIZE9], out[MSIZE9];

static uchar *pqid(uchar *p, int node){
	return putqid(p, node == 0 ? QTDIR9 : QTFILE9, node+100);
}
static void sendmsg(int type, int tag, uchar *end){ send9msg(1, type, tag, out, end); }
static void rerror(int tag, char *e){ send9err(1, tag, e, out); }

static uchar *
pstat(uchar *p, int node)
{
	static char *names[] = { "/", "motd", "note" };
	uchar *sz = p;

	p = put16(p, 0);				/* record size, patched below */
	p = put16(p, 0); p = put32(p, 0);		/* type, dev */
	p = pqid(p, node);
	p = put32(p, node == 0 ? 0x80000000|0755 : node == 2 ? 0666 : 0444);
	p = put32(p, 0); p = put32(p, 0);		/* atime, mtime */
	p = put64(p, node == 1 ? sizeof motd - 1 : node == 2 ? notelen : 0);
	p = putstr(p, names[node]);
	p = putstr(p, "hellofs"); p = putstr(p, "hellofs"); p = putstr(p, "hellofs");
	put16(sz, p - sz - 2);
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
		n = read9msg(0, msg);
		if(n == 0)
			exits(nil);		/* client side gone */
		if(n < 0)
			exits("bad message");
		type = msg[4];
		tag = get16(msg+5);
		b = msg+7;
		p = out+7;
		switch(type){
		case Tversion:
			count = get32(b);
			p = put32(p, count < MSIZE9 ? count : MSIZE9);
			p = putstr(p, "9P2000");
			sendmsg(type+1, tag, p);
			break;
		case Tattach:
			fid = get32(b);
			nf = findfid(fid, 1);
			if(nf < 0){ rerror(tag, "out of fids"); break; }
			fidnode[nf] = 0;
			p = pqid(p, 0);
			sendmsg(type+1, tag, p);
			break;
		case Twalk: {
			int newfid = get32(b+4), nname = get16(b+8);
			char name[64];
			uint ln;

			nf = findfid(fid = get32(b), 0);
			if(nf < 0){ rerror(tag, "unknown fid"); break; }
			node = fidnode[nf];
			if(nname == 0){
				nf = findfid(newfid, 1);
				fidnode[nf] = node;
				p = put16(p, 0);
				sendmsg(type+1, tag, p);
				break;
			}
			if(nname != 1){ rerror(tag, "one name per walk here"); break; }
			ln = get16(b+10);
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
			p = put16(p, 1);
			p = pqid(p, node);
			sendmsg(type+1, tag, p);
			break;
		}
		case Topen:
			nf = findfid(get32(b), 0);
			if(nf < 0){ rerror(tag, "unknown fid"); break; }
			p = pqid(p, fidnode[nf]);
			p = put32(p, 0);		/* iounit */
			sendmsg(type+1, tag, p);
			break;
		case Tread: {
			uchar dir[512], *d, *e;
			nf = findfid(get32(b), 0);
			if(nf < 0){ rerror(tag, "unknown fid"); break; }
			node = fidnode[nf];
			off = get64(b+4);
			count = get32(b+12);
			if(count > MSIZE9-24) count = MSIZE9-24;
			if(node == 0){
				/* whole listing, then an integral slice, per read(5) */
				d = pstat(dir, 1);
				e = pstat(d, 2);
				n = 0;
				d = dir;
				while(d < e && (uvlong)(d-dir) < off)
					d += get16(d)+2;
				while(d+ (get16(d)+2) <= e && n + get16(d)+2 <= count && d < e){
					memcpy(p+4+n, d, get16(d)+2);
					n += get16(d)+2;
					d += get16(d)+2;
					if(d >= e) break;
				}
				put32(p, n);
				p += 4+n;
			} else {
				char *src = node == 1 ? motd : note;
				uint len = node == 1 ? sizeof motd - 1 : (uint)notelen;
				if(off > len) off = len;
				n = len - (uint)off;
				if(n > count) n = count;
				put32(p, n);
				memcpy(p+4, src+off, n);
				p += 4+n;
			}
			sendmsg(type+1, tag, p);
			break;
		}
		case Twrite:
			nf = findfid(get32(b), 0);
			if(nf < 0){ rerror(tag, "unknown fid"); break; }
			if(fidnode[nf] != 2){ rerror(tag, "read-only file"); break; }
			off = get64(b+4);
			count = get32(b+12);
			if(off + count > sizeof note){ rerror(tag, "note is small"); break; }
			memcpy(note+off, b+16, count);
			if((int)(off+count) > notelen)
				notelen = off+count;
			p = put32(p, count);
			sendmsg(type+1, tag, p);
			break;
		case Tclunk:
		case Tremove:			/* remove also clunks; we refuse the removal part */
			nf = findfid(get32(b), 0);
			if(nf >= 0)
				fids[nf] = -1;
			if(type == Tremove){ rerror(tag, "not removable"); break; }
			sendmsg(type+1, tag, p);
			break;
		case Tstat: {
			uchar *sz;
			nf = findfid(get32(b), 0);
			if(nf < 0){ rerror(tag, "unknown fid"); break; }
			sz = p;			/* stat(5): the record, twice counted */
			p = put16(p, 0);
			p = pstat(p, fidnode[nf]);
			put16(sz, p - sz - 2);
			sendmsg(type+1, tag, p);
			break;
		}
		default:
			rerror(tag, "hellofs: unsupported request");
		}
	}
	return 0;
}
