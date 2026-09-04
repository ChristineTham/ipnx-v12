/* exportfs: serve THIS PROCESS'S NAMESPACE over wire 9P on fd 0 — the
 * reverse of the kernel's mount driver, and the second half of the design's
 * boundary story: a namespace is a value you can hand to someone else.
 * Every request is answered with real system calls against our own view,
 * so what travels is exactly what this process was arranged to see —
 * binds included. Errors relay as Rerror carrying our errstr.
 *
 * exportfs [root]  (default /) */
#include "lib9.h"
#include "lib9p.h"

enum { NFID = 64, MAXPATH = 512 };

typedef struct Fid Fid;
struct Fid {
	int	fid;		/* -1 = free */
	int	fd;		/* -1 until opened */
	char	path[MAXPATH];
};
static Fid fids[NFID];
static char *root;
static uchar msg[MSIZE9], out[MSIZE9];

static Fid *
findfid(int fid, int alloc)
{
	Fid *f, *free_ = nil;
	int i;

	for(i = 0; i < NFID; i++){
		f = &fids[i];
		if(f->fid == fid)
			return f;
		if(f->fid == -1 && free_ == nil)
			free_ = f;
	}
	if(alloc && free_ != nil){
		free_->fid = fid;
		free_->fd = -1;
		return free_;
	}
	return nil;
}

static void
clunkfid(Fid *f)
{
	if(f->fd >= 0)
		close(f->fd);
	f->fid = -1;
	f->fd = -1;
}

/* qid from a stat record: size2 type2 dev4, then qid.type[1] vers[4] path[8] */
static uchar *
statqid(uchar *edir, uchar *p)
{
	memcpy(p, edir+8, 13);
	return p+13;
}

static void
rerr(int tag)
{
	char e[128];

	errstr(e, sizeof e);
	send9err(1, tag, e[0] ? e : "exportfs: error", out);
}

int
main(int argc, char *argv[])
{
	uchar edir[512], *b, *p;
	char name[128], path[MAXPATH];
	long n;
	int i, type, tag;
	uint count;
	uvlong off;
	Fid *f, *nf;

	root = argc > 1 ? argv[1] : "/";
	dup(0, 1);
	for(i = 0; i < NFID; i++)
		fids[i].fid = -1;
	for(;;){
		n = read9msg(0, msg);
		if(n == 0)
			exits(nil);
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
			send9msg(1, type+1, tag, out, p);
			break;
		case Tattach:
			f = findfid(get32(b), 1);
			if(f == nil){ send9err(1, tag, "out of fids", out); break; }
			strcpy(f->path, root);
			if(stat(f->path, edir, sizeof edir) < 0){ clunkfid(f); rerr(tag); break; }
			p = statqid(edir, p);
			send9msg(1, type+1, tag, out, p);
			break;
		case Twalk: {
			int newfid = get32(b+4), nname = get16(b+8);
			uint ln;

			f = findfid(get32(b), 0);
			if(f == nil){ send9err(1, tag, "unknown fid", out); break; }
			if(nname > 1){ send9err(1, tag, "one name per walk here", out); break; }
			strcpy(path, f->path);
			if(nname == 1){
				ln = get16(b+10);
				if(ln > 100){ send9err(1, tag, "name too long", out); break; }
				memcpy(name, b+12, ln);
				name[ln] = 0;
				if(strlen(path)+ln+2 > MAXPATH){ send9err(1, tag, "path too long", out); break; }
				if(strcmp(path, "/") != 0)
					strcpy(path+strlen(path), "/");
				strcpy(path+strlen(path), name);
				if(stat(path, edir, sizeof edir) < 0){ rerr(tag); break; }
			}
			nf = findfid(newfid, 1);
			if(nf == nil){ send9err(1, tag, "out of fids", out); break; }
			strcpy(nf->path, path);
			p = put16(p, nname);
			if(nname == 1)
				p = statqid(edir, p);
			send9msg(1, type+1, tag, out, p);
			break;
		}
		case Topen:
			f = findfid(get32(b), 0);
			if(f == nil){ send9err(1, tag, "unknown fid", out); break; }
			f->fd = open(f->path, b[4]);
			if(f->fd < 0){ rerr(tag); break; }
			if(stat(f->path, edir, sizeof edir) < 0){ rerr(tag); break; }
			p = statqid(edir, p);
			p = put32(p, 0);
			send9msg(1, type+1, tag, out, p);
			break;
		case Tcreate: {
			uint ln = get16(b+4);

			f = findfid(get32(b), 0);
			if(f == nil){ send9err(1, tag, "unknown fid", out); break; }
			memcpy(name, b+6, ln);
			name[ln] = 0;
			strcpy(path, f->path);
			if(strcmp(path, "/") != 0)
				strcpy(path+strlen(path), "/");
			strcpy(path+strlen(path), name);
			f->fd = create(path, b[6+ln+4], get32(b+6+ln));
			if(f->fd < 0){ rerr(tag); break; }
			strcpy(f->path, path);
			if(stat(path, edir, sizeof edir) < 0){ rerr(tag); break; }
			p = statqid(edir, p);
			p = put32(p, 0);
			send9msg(1, type+1, tag, out, p);
			break;
		}
		case Tread:
			f = findfid(get32(b), 0);
			if(f == nil || f->fd < 0){ send9err(1, tag, "not open", out); break; }
			off = get64(b+4);
			count = get32(b+12);
			if(count > MSIZE9-24)
				count = MSIZE9-24;
			n = pread(f->fd, p+4, count, off);
			if(n < 0){ rerr(tag); break; }
			p = put32(p, n);
			p += n;
			send9msg(1, type+1, tag, out, p);
			break;
		case Twrite:
			f = findfid(get32(b), 0);
			if(f == nil || f->fd < 0){ send9err(1, tag, "not open", out); break; }
			off = get64(b+4);
			count = get32(b+12);
			n = pwrite(f->fd, b+16, count, off);
			if(n < 0){ rerr(tag); break; }
			p = put32(p, n);
			send9msg(1, type+1, tag, out, p);
			break;
		case Tremove:
			f = findfid(get32(b), 0);
			if(f == nil){ send9err(1, tag, "unknown fid", out); break; }
			n = remove(f->path);
			clunkfid(f);
			if(n < 0){ rerr(tag); break; }
			send9msg(1, type+1, tag, out, p);
			break;
		case Tclunk:
			f = findfid(get32(b), 0);
			if(f != nil)
				clunkfid(f);
			send9msg(1, type+1, tag, out, p);
			break;
		case Tstat:
			f = findfid(get32(b), 0);
			if(f == nil){ send9err(1, tag, "unknown fid", out); break; }
			n = stat(f->path, edir, sizeof edir);
			if(n < 0){ rerr(tag); break; }
			p = put16(p, n);	/* stat(5): the record, twice counted */
			memcpy(p, edir, n);
			p += n;
			send9msg(1, type+1, tag, out, p);
			break;
		default:
			send9err(1, tag, "exportfs: unsupported request", out);
		}
	}
	return 0;
}
