/* storefs: the store's file server (docs/type.md, decided 2026-09-04).
 *
 * `/store` holds FETCHED content — what a declaration names, verifies by
 * digest, and BINDS. Its one load-bearing property is IMMUTABILITY: "a store
 * entry never changes after verification — otherwise the digest lies and every
 * declaration referencing it becomes a false claim." The spec calls that "a
 * one-line rule rather than a convention", and this is where the line lives.
 *
 * It is a USERSPACE server over a host directory rather than that directory
 * bound in directly, so the rule is enforced by a program IPNX owns instead of
 * being a promise the host makes. That was the choice (design.md 2026-09-04);
 * pkg still verifies the digest, and pkg is ours too, so verification sits
 * inside IPNX at both ends.
 *
 * It STREAMS. Every read is a pread straight through to the backing file and
 * every write a pwrite: the server never holds an entry's bytes, because a
 * guest's linear memory caps at 16MB and one package is 29MB
 * (RESEARCH §9.24). Userspace is a budget, not a location.
 *
 *   storefs [-s name] root
 *
 * `root` is the backing directory — '#Z/store' on a host that has one. With
 * -s it posts itself at /srv/<name>; otherwise it serves on fd 0.
 *
 * The rule, exactly:
 *   - a create whose path already exists is REFUSED
 *   - an open for writing is REFUSED, always — an existing entry is closed
 *   - a write is accepted only on a fid this session created
 *   - a remove is allowed: prune discards materialisation, never intent
 */
#include "lib9.h"
#include "lib9p.h"

enum { NFID = 64, MAXPATH = 512 };

typedef struct Fid Fid;
struct Fid {
	int	fid;		/* -1 = free */
	int	fd;		/* -1 until opened */
	int	mine;		/* this session created it: writes allowed */
	char	path[MAXPATH];
};
static Fid fids[NFID];
static char *root;
static uchar msg[MSIZE9], out[MSIZE9];

static char Eimmutable[] = "store entries are immutable once written";
static char Eexists[]    = "store entry exists";

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
		free_->mine = 0;
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
	f->mine = 0;
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
	send9err(1, tag, e[0] ? e : "storefs: error", out);
}

static int
exists(char *path)
{
	uchar edir[512];

	return stat(path, edir, sizeof edir) >= 0;
}

int
main(int argc, char *argv[])
{
	uchar edir[512], *b, *p;
	char name[128], path[MAXPATH], *srvname;
	long n;
	int i, type, tag, mode;
	uint count;
	uvlong off;
	Fid *f, *nf;

	srvname = nil;
	root = nil;
	for(i = 1; i < argc; i++){
		if(strcmp(argv[i], "-s") == 0 && i+1 < argc)
			srvname = argv[++i];
		else
			root = argv[i];
	}
	if(root == nil){
		fprint(2, "usage: storefs [-s name] root\n");
		exits("usage");
	}

	for(i = 0; i < NFID; i++)
		fids[i].fid = -1;

	if(srvname != nil){
		char sp[128];
		int sfd, pfd[2];

		if(pipe(pfd) < 0){
			fprint(2, "storefs: pipe: %r\n");
			exits("pipe");
		}
		snprint(sp, sizeof sp, "/srv/%s", srvname);
		sfd = create(sp, OWRITE, 0600);
		if(sfd < 0){
			fprint(2, "storefs: %s: %r\n", sp);
			exits("srv");
		}
		fprint(sfd, "%d", pfd[1]);
		close(sfd);
		close(pfd[1]);
		dup(pfd[0], 0);
		close(pfd[0]);
	}
	dup(0, 1);

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
			mode = b[4];
			/* THE RULE: an entry that exists is closed to writing. */
			if((mode & 3) != OREAD || (mode & OTRUNC) != 0){
				send9err(1, tag, Eimmutable, out);
				break;
			}
			f->fd = open(f->path, mode);
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
			/* THE RULE, the other half: never over an existing entry. */
			if(exists(path)){ send9err(1, tag, Eexists, out); break; }
			f->fd = create(path, b[6+ln+4], get32(b+6+ln));
			if(f->fd < 0){ rerr(tag); break; }
			f->mine = 1;			/* ours this session: writable */
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
			n = pread(f->fd, p+4, count, off);	/* straight through: never held */
			if(n < 0){ rerr(tag); break; }
			p = put32(p, n);
			p += n;
			send9msg(1, type+1, tag, out, p);
			break;

		case Twrite:
			f = findfid(get32(b), 0);
			if(f == nil || f->fd < 0){ send9err(1, tag, "not open", out); break; }
			if(!f->mine){ send9err(1, tag, Eimmutable, out); break; }
			off = get64(b+4);
			count = get32(b+12);
			n = pwrite(f->fd, b+16, count, off);
			if(n < 0){ rerr(tag); break; }
			p = put32(p, n);
			send9msg(1, type+1, tag, out, p);
			break;

		case Tremove:				/* prune: materialisation, never intent */
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

		case Tstat: {
			uchar *sz;

			f = findfid(get32(b), 0);
			if(f == nil){ send9err(1, tag, "unknown fid", out); break; }
			if(stat(f->path, edir, sizeof edir) < 0){ rerr(tag); break; }
			n = get16(edir) + 2;
			sz = p;
			p = put16(p, 0);
			memcpy(p, edir, n);
			p += n;
			put16(sz, p - sz - 2);
			send9msg(1, type+1, tag, out, p);
			break;
		}

		case Twstat:
			send9err(1, tag, Eimmutable, out);
			break;

		default:
			send9err(1, tag, "storefs: unsupported request", out);
		}
	}
	return 0;
}
