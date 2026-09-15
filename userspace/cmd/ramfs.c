/* ramfs: the root file server, in userspace (implementation.md P2 step 2).
 *
 * Plan 9 splits two roles that this kernel's '#R' still combines: the
 * BOOTSTRAP root is a kernel device (`#/`, devroot.c — landed as P2 step 1),
 * and the REAL filesystem is a user program. This is that program, and
 * plan9/sys/src/cmd/ramfs.c (945 lines in 9legacy) is its reference shape:
 * a tree held in memory, served over 9P2000, with the tree therefore a
 * PROCESS rather than kernel state.
 *
 * Serving on fd 0, as hellofs and exportfs do — the wire boundary the Dev
 * table stops at. `ramfs -s NAME` posts itself in /srv instead.
 *
 *   ramfs [-s name] [seed]
 *
 * `seed` is a directory copied in at startup, and it is how "the host stays
 * a storage box and the tree is a process" is meant to work: the host's
 * directory arrives as '#Z' and this server reads it through the namespace
 * like any other tree. Absent, the server starts empty.
 *
 * WHAT THIS CANNOT YET DO, MEASURED (RESEARCH §9.24): hold the real rootfs.
 * It is 49MB — bin/python alone is 29MB — and a guest's linear memory is
 * capped at 16MB by the host. So this server is the right shape and the
 * wrong size until P2 step 5 moves Go and Python out of the rootfs and into
 * packages. Seeding a small tree works today and is what the suite proves.
 */
#include "lib9.h"
#include "lib9p.h"

enum {
	NFID	= 64,
	NRAM	= 512,		/* nodes; the seed's tree must fit */
	NAMELEN	= 64,
	CHUNK	= 8192,
};

typedef struct Ram Ram;
struct Ram {
	char	name[NAMELEN];
	int	isdir;
	int	inuse;
	Ram	*parent;
	Ram	*kid;		/* first child */
	Ram	*next;		/* next sibling */
	uchar	*data;
	uint	len;
	uint	cap;
	ulong	perm;
	uvlong	qpath;
};

static Ram rams[NRAM];
static int nram;
static uvlong qgen = 1;

static int fids[NFID];
static Ram *fidram[NFID];

static uchar msg[MSIZE9], out[MSIZE9];
static char *srvname;

static char Eperm[]	= "permission denied";
static char Enotdir[]	= "not a directory";
static char Enotexist[]	= "file does not exist";
static char Eexist[]	= "file exists";
static char Eisdir[]	= "file is a directory";
static char Efull[]	= "ramfs: out of nodes";
static char Enomem[]	= "ramfs: out of memory";
static char Enotempty[]	= "directory not empty";
static char Ebadfid[]	= "unknown fid";

static void sendmsg(int type, int tag, uchar *end){ send9msg(1, type, tag, out, end); }
static void rerror(int tag, char *e){ send9err(1, tag, e, out); }

static Ram *
newram(Ram *parent, char *name, int isdir, ulong perm)
{
	Ram *r, **pp;

	if(nram >= NRAM)
		return nil;
	r = &rams[nram++];
	r->inuse = 1;
	strncpy(r->name, name, NAMELEN-1);
	r->name[NAMELEN-1] = 0;
	r->isdir = isdir;
	r->parent = parent;
	r->kid = nil;
	r->next = nil;
	r->data = nil;
	r->len = r->cap = 0;
	r->perm = perm;
	r->qpath = qgen++;
	if(parent != nil){
		for(pp = &parent->kid; *pp != nil; pp = &(*pp)->next)
			;
		*pp = r;			/* insertion order, like the kernel's ramfs */
	}
	return r;
}

static Ram *
walk1(Ram *d, char *name)
{
	Ram *r;

	if(strcmp(name, "..") == 0)
		return d->parent != nil ? d->parent : d;
	for(r = d->kid; r != nil; r = r->next)
		if(r->inuse && strcmp(r->name, name) == 0)
			return r;
	return nil;
}

static int
grow(Ram *r, uint need)
{
	uchar *p;
	uint cap;

	if(need <= r->cap)
		return 1;
	cap = r->cap ? r->cap : CHUNK;
	while(cap < need)
		cap *= 2;
	p = malloc(cap);
	if(p == nil)
		return 0;
	if(r->data != nil){
		memcpy(p, r->data, r->len);
		free(r->data);
	}
	r->data = p;
	r->cap = cap;
	return 1;
}

static uchar *
pqid(uchar *p, Ram *r)
{
	return putqid(p, r->isdir ? QTDIR9 : QTFILE9, r->qpath);
}

static uchar *
pstat(uchar *p, Ram *r)
{
	uchar *sz = p;

	p = put16(p, 0);				/* record size, patched below */
	p = put16(p, 0); p = put32(p, 0);		/* type, dev */
	p = pqid(p, r);
	p = put32(p, r->isdir ? (0x80000000UL|r->perm) : r->perm);
	p = put32(p, 0); p = put32(p, 0);		/* atime, mtime */
	p = put64(p, r->isdir ? 0 : r->len);
	p = putstr(p, r->name);
	p = putstr(p, "ramfs"); p = putstr(p, "ramfs"); p = putstr(p, "ramfs");
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

/* ---- seeding: copy a directory in through the namespace ---- */

static void
seed(Ram *dir, char *path)
{
	uchar edir[512];
	char sub[512], name[NAMELEN], *buf;
	int fd, n;
	Ram *r;

	fd = open(path, OREAD);
	if(fd < 0){
		fprint(2, "ramfs: %s: %r\n", path);
		return;
	}
	while((n = read(fd, edir, sizeof edir)) > 0){
		uchar *e = edir, *end = edir + n;

		while(e < end){
			uint rec = get16(e) + 2;

			if(rec < 2 || e + rec > end)
				break;
			statname(e, name, sizeof name);
			snprint(sub, sizeof sub, "%s/%s", path, name);
			if(statmode(e) & DMDIR){
				r = newram(dir, name, 1, 0775);
				if(r == nil){ fprint(2, "ramfs: %s\n", Efull); close(fd); return; }
				seed(r, sub);
			} else {
				int ff, k;

				r = newram(dir, name, 0, 0666);
				if(r == nil){ fprint(2, "ramfs: %s\n", Efull); close(fd); return; }
				ff = open(sub, OREAD);
				if(ff < 0){
					fprint(2, "ramfs: %s: %r\n", sub);
				} else {
					/* stream: the whole file never exists twice */
					while(grow(r, r->len + CHUNK)){
						buf = (char*)r->data + r->len;
						k = read(ff, buf, CHUNK);
						if(k <= 0)
							break;
						r->len += k;
					}
					close(ff);
				}
			}
			e += rec;
		}
	}
	close(fd);
}

/* ---- the server loop ---- */

int
main(int argc, char *argv[])
{
	int i, type, tag, fid, nf;
	uchar *p, *b;
	uint n, count;
	uvlong off;
	Ram *root, *r;
	char *seedpath = nil;

	for(i = 1; i < argc; i++){
		if(strcmp(argv[i], "-s") == 0 && i+1 < argc)
			srvname = argv[++i];
		else
			seedpath = argv[i];
	}

	for(i = 0; i < NFID; i++)
		fids[i] = -1;
	root = newram(nil, "/", 1, 0775);
	if(seedpath != nil)
		seed(root, seedpath);

	if(srvname != nil){
		char path[128];
		int sfd, pfd[2];

		if(pipe(pfd) < 0){
			fprint(2, "ramfs: pipe: %r\n");
			exits("pipe");
		}
		snprint(path, sizeof path, "/srv/%s", srvname);
		sfd = create(path, OWRITE, 0600);
		if(sfd < 0){
			fprint(2, "ramfs: %s: %r\n", path);
			exits("srv");
		}
		fprint(sfd, "%d", pfd[1]);
		close(sfd);
		close(pfd[1]);
		dup(pfd[0], 0);
		close(pfd[0]);
	}
	dup(0, 1);				/* serve both ways on the pipe */

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
			fidram[nf] = root;
			p = pqid(p, root);
			sendmsg(type+1, tag, p);
			break;

		case Twalk: {
			int newfid = get32(b+4), nname = get16(b+8), w;
			uchar *q = b+10;
			Ram *at;
			char name[NAMELEN];
			uchar *nw;

			nf = findfid(get32(b), 0);
			if(nf < 0){ rerror(tag, Ebadfid); break; }
			at = fidram[nf];
			nw = p;
			p = put16(p, 0);		/* nwqid, patched below */
			for(w = 0; w < nname; w++){
				uint ln = get16(q);

				if(ln >= NAMELEN){ break; }
				memcpy(name, q+2, ln);
				name[ln] = 0;
				q += 2 + ln;
				if(!at->isdir){ at = nil; break; }
				at = walk1(at, name);
				if(at == nil)
					break;
				p = pqid(p, at);
			}
			if(w == 0 && nname > 0){
				rerror(tag, Enotexist);
				break;
			}
			put16(nw, w);
			if(w == nname){
				nf = findfid(newfid, 1);
				if(nf < 0){ rerror(tag, "out of fids"); break; }
				fidram[nf] = at;
			}
			sendmsg(type+1, tag, p);
			break;
		}

		case Topen:
			nf = findfid(get32(b), 0);
			if(nf < 0){ rerror(tag, Ebadfid); break; }
			p = pqid(p, fidram[nf]);
			p = put32(p, 0);		/* iounit */
			sendmsg(type+1, tag, p);
			break;

		case Tcreate: {
			uint ln = get16(b+4);
			ulong perm;
			char name[NAMELEN];

			nf = findfid(get32(b), 0);
			if(nf < 0){ rerror(tag, Ebadfid); break; }
			if(ln >= NAMELEN){ rerror(tag, "name too long"); break; }
			memcpy(name, b+6, ln);
			name[ln] = 0;
			perm = get32(b+6+ln);
			r = fidram[nf];
			if(!r->isdir){ rerror(tag, Enotdir); break; }
			if(walk1(r, name) != nil){ rerror(tag, Eexist); break; }
			r = newram(r, name, (perm & DMDIR) != 0, perm & 0777);
			if(r == nil){ rerror(tag, Efull); break; }
			fidram[nf] = r;
			p = pqid(p, r);
			p = put32(p, 0);		/* iounit */
			sendmsg(type+1, tag, p);
			break;
		}

		case Tread: {
			uchar dir[MSIZE9], *d, *e;

			nf = findfid(get32(b), 0);
			if(nf < 0){ rerror(tag, Ebadfid); break; }
			r = fidram[nf];
			off = get64(b+4);
			count = get32(b+12);
			if(count > MSIZE9-24) count = MSIZE9-24;
			if(r->isdir){
				Ram *k;

				/* the whole listing, then an integral slice (read(5)) */
				e = dir;
				for(k = r->kid; k != nil; k = k->next)
					if(k->inuse && (uint)(e - dir) < sizeof dir - 512)
						e = pstat(e, k);
				d = dir;
				while(d < e && (uvlong)(d - dir) < off)
					d += get16(d) + 2;
				n = 0;
				while(d < e){
					uint rec = get16(d) + 2;

					if(n + rec > count)
						break;
					memcpy(p+4+n, d, rec);
					n += rec;
					d += rec;
				}
				put32(p, n);
				p += 4 + n;
			} else {
				if(off > r->len) off = r->len;
				n = r->len - (uint)off;
				if(n > count) n = count;
				put32(p, n);
				if(n > 0)
					memcpy(p+4, r->data + off, n);
				p += 4 + n;
			}
			sendmsg(type+1, tag, p);
			break;
		}

		case Twrite:
			nf = findfid(get32(b), 0);
			if(nf < 0){ rerror(tag, Ebadfid); break; }
			r = fidram[nf];
			if(r->isdir){ rerror(tag, Eisdir); break; }
			off = get64(b+4);
			count = get32(b+12);
			if(off + count > 0xffffffffULL){ rerror(tag, Enomem); break; }
			if(!grow(r, (uint)off + count)){ rerror(tag, Enomem); break; }
			if(off > r->len)
				memset(r->data + r->len, 0, (uint)off - r->len);
			memcpy(r->data + off, b+16, count);
			if((uint)off + count > r->len)
				r->len = (uint)off + count;
			p = put32(p, count);
			sendmsg(type+1, tag, p);
			break;

		case Tclunk:
			nf = findfid(get32(b), 0);
			if(nf >= 0)
				fids[nf] = -1;
			sendmsg(type+1, tag, p);
			break;

		case Tremove: {
			Ram **pp;

			nf = findfid(get32(b), 0);
			if(nf < 0){ rerror(tag, Ebadfid); break; }
			r = fidram[nf];
			fids[nf] = -1;			/* remove also clunks */
			if(r->parent == nil){ rerror(tag, Eperm); break; }
			if(r->isdir && r->kid != nil){
				Ram *k;
				int any = 0;

				for(k = r->kid; k != nil; k = k->next)
					if(k->inuse)
						any = 1;
				if(any){ rerror(tag, Enotempty); break; }
			}
			for(pp = &r->parent->kid; *pp != nil; pp = &(*pp)->next)
				if(*pp == r){
					*pp = r->next;
					break;
				}
			r->inuse = 0;
			if(r->data != nil){
				free(r->data);
				r->data = nil;
				r->len = r->cap = 0;
			}
			sendmsg(type+1, tag, p);
			break;
		}

		case Tstat: {
			uchar *sz;

			nf = findfid(get32(b), 0);
			if(nf < 0){ rerror(tag, Ebadfid); break; }
			sz = p;				/* stat(5): the record, twice counted */
			p = put16(p, 0);
			p = pstat(p, fidram[nf]);
			put16(sz, p - sz - 2);
			sendmsg(type+1, tag, p);
			break;
		}

		default:
			rerror(tag, "ramfs: unsupported request");
		}
	}
	return 0;
}
