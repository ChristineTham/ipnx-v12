/* svc: the control plane, as a file server — M12's local stage
 * (design.md 2026-08-30: "the orchestrator is a file server, kubectl is
 * cat and echo"). Serves 9P on a pipe posted at /srv/svc:
 *
 *   /ctl              start name specdir [n] | scale name n | stop name
 *   /<name>/status    "want N have M pids P1 P2…" — reading reconciles
 *
 * Replicas are spawned RFNOWAIT through run(1) — no zombies by
 * construction — and liveness is a walk: a pid whose /proc/<pid>/status
 * no longer opens is dead. Reconciliation runs on every ctl write and
 * status read, and on an alarm tick while idle (the tick's note
 * interrupts the blocked read; a tick landing mid-message would desync
 * the stream — the window is the gap between a request's first byte and
 * its last, accepted for v0). Desired state is files you write;
 * observation is files you read.
 */
#include "lib9.h"
#include "lib9p.h"

enum {
	NSVC = 8,
	NREP = 8,
	NFID = 32,
	TICK = 2000,
};

typedef struct Svc Svc;
struct Svc {
	int used;
	char name[28];
	char spec[120];
	int want;
	int pids[NREP];
	int npid;
};

static Svc svcs[NSVC];
static int fids[NFID], fidnode[NFID];
static int sfd;
static uchar msg[MSIZE9], out[MSIZE9];

/* nodes: 0 root, 1 ctl, service i: dir 10+2i, status 11+2i */
static int
isdirnode(int node)
{
	return node == 0 || (node >= 10 && (node & 1) == 0);
}

static uchar *
pqid(uchar *p, int node)
{
	return putqid(p, isdirnode(node) ? QTDIR9 : QTFILE9, node + 200);
}

static void sendreply(int type, int tag, uchar *end){ send9msg(sfd, type, tag, out, end); }
static void rerror(int tag, char *e){ send9err(sfd, tag, e, out); }

static void
notehandler(void *v, char *s)
{
	USED(v); USED(s);
	noted(NCONT);			/* the tick: just interrupt the read */
}

static void
repchild(void *v)
{
	Svc *s = v;
	char *av[3];

	dup(2, 0);
	dup(2, 1);
	av[0] = "run"; av[1] = s->spec; av[2] = nil;
	exec("/bin/run", av);
	fprint(2, "svc: exec /bin/run: %r\n");
	exits("exec");
}

static int
alive(int pid)
{
	char path[64];
	int fd;

	snprint(path, sizeof path, "/proc/%d/status", pid);
	fd = open(path, OREAD);
	if(fd < 0)
		return 0;
	close(fd);
	return 1;
}

static void
reconcile(void)
{
	Svc *s;
	int i, j, k, pid;

	for(i = 0; i < NSVC; i++){
		s = &svcs[i];
		if(!s->used)
			continue;
		for(j = k = 0; j < s->npid; j++)	/* drop the dead */
			if(alive(s->pids[j]))
				s->pids[k++] = s->pids[j];
		s->npid = k;
		while(s->npid > s->want){		/* shrink: newest first */
			postnote(PNPROC, s->pids[s->npid - 1], "kill");
			s->npid--;
		}
		while(s->npid < s->want && s->npid < NREP){
			pid = procrfork(RFFDG | RFNOWAIT, repchild, s);
			if(pid <= 0)
				break;
			s->pids[s->npid++] = pid;
		}
		if(s->want == 0 && s->npid == 0)
			s->used = 0;			/* stopped and drained */
	}
}

static Svc *
findsvc(char *name)
{
	int i;

	for(i = 0; i < NSVC; i++)
		if(svcs[i].used && strcmp(svcs[i].name, name) == 0)
			return &svcs[i];
	return nil;
}

static int
mkstatus(Svc *s, char *buf, int max)
{
	int j, n;

	n = snprint(buf, max, "want %d have %d pids", s->want, s->npid);
	for(j = 0; j < s->npid; j++)
		n += snprint(buf + n, max - n, " %d", s->pids[j]);
	n += snprint(buf + n, max - n, "\n");
	return n;
}

static uchar *
pstat(uchar *p, char *name, int node)
{
	uchar *sz = p;

	p = put16(p, 0);
	p = put16(p, 0); p = put32(p, 0);
	p = pqid(p, node);
	p = put32(p, isdirnode(node) ? (0x80000000 | 0555) : node == 1 ? 0666 : 0444);
	p = put32(p, 0); p = put32(p, 0);
	p = put64(p, 0);
	p = putstr(p, name);
	p = putstr(p, "svc"); p = putstr(p, "svc"); p = putstr(p, "svc");
	put16(sz, p - sz - 2);
	return p;
}

/* ctl: start name specdir [n] | scale name n | stop name */
static char *
ctl(char *line)
{
	char *f[5];
	int nf, i, n;
	Svc *s;
	char *p;

	nf = 0;
	for(p = line; *p != 0 && nf < 5; ){
		while(*p == ' ' || *p == '\t' || *p == '\n')
			*p++ = 0;
		if(*p == 0)
			break;
		f[nf++] = p;
		while(*p != 0 && *p != ' ' && *p != '\t' && *p != '\n')
			p++;
	}
	if(nf < 2)
		return "usage: start name spec [n] | scale name n | stop name";
	if(strcmp(f[0], "start") == 0){
		if(nf < 3)
			return "start wants a spec directory";
		if(findsvc(f[1]) != nil)
			return "service exists";
		for(i = 0; i < NSVC && svcs[i].used; i++)
			;
		if(i == NSVC)
			return "service table full";
		s = &svcs[i];
		memset(s, 0, sizeof *s);
		snprint(s->name, sizeof s->name, "%s", f[1]);
		snprint(s->spec, sizeof s->spec, "%s", f[2]);
		n = 1;
		if(nf > 3){
			n = 0;
			for(p = f[3]; *p >= '0' && *p <= '9'; p++)
				n = n * 10 + (*p - '0');
		}
		s->want = n > NREP ? NREP : n;
		s->used = 1;
	} else if(strcmp(f[0], "scale") == 0){
		if(nf < 3)
			return "scale wants a count";
		s = findsvc(f[1]);
		if(s == nil)
			return "no such service";
		n = 0;
		for(p = f[2]; *p >= '0' && *p <= '9'; p++)
			n = n * 10 + (*p - '0');
		s->want = n > NREP ? NREP : n;
	} else if(strcmp(f[0], "stop") == 0){
		s = findsvc(f[1]);
		if(s == nil)
			return "no such service";
		s->want = 0;
	} else
		return "unknown verb";
	reconcile();
	return nil;
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
	int i, type, tag, fid, nf, node, pfd[2], postfd;
	uchar *p, *b;
	uint n, count;
	uvlong off;
	char sbuf[512], *e;
	long rn;

	USED(argc); USED(argv);
	for(i = 0; i < NFID; i++)
		fids[i] = -1;
	notify(notehandler);

	if(pipe(pfd) < 0)
		exits("pipe");
	postfd = create("/srv/svc", OWRITE, 0600);
	if(postfd < 0){
		fprint(2, "svc: /srv/svc: %r\n");
		exits("post");
	}
	fprint(postfd, "%d", pfd[0]);
	close(postfd);
	close(pfd[0]);
	sfd = pfd[1];

	for(;;){
		alarm(TICK);
		rn = read9msg(sfd, msg);
		alarm(0);
		if(rn < 0){			/* the tick (or a torn read) */
			reconcile();
			continue;
		}
		if(rn == 0)
			exits(nil);		/* every client end gone */
		type = msg[4];
		tag = get16(msg + 5);
		b = msg + 7;
		p = out + 7;
		switch(type){
		case Tversion:
			count = get32(b);
			p = put32(p, count < MSIZE9 ? count : MSIZE9);
			p = putstr(p, "9P2000");
			sendreply(type + 1, tag, p);
			break;
		case Tattach:
			fid = get32(b);
			nf = findfid(fid, 1);
			if(nf < 0){ rerror(tag, "out of fids"); break; }
			fidnode[nf] = 0;
			p = pqid(p, 0);
			sendreply(type + 1, tag, p);
			break;
		case Twalk: {
			int newfid = get32(b + 4), nname = get16(b + 8);
			char name[64];
			uint ln;
			Svc *s;

			nf = findfid(fid = get32(b), 0);
			if(nf < 0){ rerror(tag, "unknown fid"); break; }
			node = fidnode[nf];
			if(nname == 0){
				nf = findfid(newfid, 1);
				fidnode[nf] = node;
				p = put16(p, 0);
				sendreply(type + 1, tag, p);
				break;
			}
			if(nname != 1){ rerror(tag, "one name per walk here"); break; }
			ln = get16(b + 10);
			if(ln > 63){ rerror(tag, "name too long"); break; }
			memcpy(name, b + 12, ln);
			name[ln] = 0;
			if(node == 0){
				if(strcmp(name, "ctl") == 0)
					node = 1;
				else if((s = findsvc(name)) != nil)
					node = 10 + 2 * (int)(s - svcs);
				else { rerror(tag, "file not found"); break; }
			} else if(node >= 10 && (node & 1) == 0 && strcmp(name, "status") == 0)
				node++;
			else { rerror(tag, "file not found"); break; }
			nf = findfid(newfid, 1);
			if(nf < 0){ rerror(tag, "out of fids"); break; }
			fidnode[nf] = node;
			p = put16(p, 1);
			p = pqid(p, node);
			sendreply(type + 1, tag, p);
			break;
		}
		case Topen:
			nf = findfid(get32(b), 0);
			if(nf < 0){ rerror(tag, "unknown fid"); break; }
			p = pqid(p, fidnode[nf]);
			p = put32(p, 0);
			sendreply(type + 1, tag, p);
			break;
		case Tread: {
			uchar dir[MSIZE9], *d, *e2;

			nf = findfid(get32(b), 0);
			if(nf < 0){ rerror(tag, "unknown fid"); break; }
			node = fidnode[nf];
			off = get64(b + 4);
			count = get32(b + 12);
			if(count > MSIZE9 - 24)
				count = MSIZE9 - 24;
			if(node == 0){
				reconcile();		/* the listing tells the truth */
				d = pstat(dir, "ctl", 1);
				for(i = 0; i < NSVC; i++)
					if(svcs[i].used)
						d = pstat(d, svcs[i].name, 10 + 2 * i);
				e2 = d;
				n = 0;
				d = dir;
				while(d < e2 && (uvlong)(d - dir) < off)
					d += get16(d) + 2;
				while(d < e2 && d + get16(d) + 2 <= e2 && n + get16(d) + 2 <= count){
					memcpy(p + 4 + n, d, get16(d) + 2);
					n += get16(d) + 2;
					d += get16(d) + 2;
				}
				put32(p, n);
				p += 4 + n;
			} else if(node >= 11 && (node & 1) == 1){
				Svc *s = &svcs[(node - 11) / 2];
				uint len;

				if(!s->used){ rerror(tag, "service gone"); break; }
				reconcile();		/* reading IS the probe */
				len = mkstatus(s, sbuf, sizeof sbuf);
				if(off > len)
					off = len;
				n = len - (uint)off;
				if(n > count)
					n = count;
				put32(p, n);
				memcpy(p + 4, sbuf + off, n);
				p += 4 + n;
			} else {			/* ctl and service dirs read empty */
				put32(p, 0);
				p += 4;
			}
			sendreply(type + 1, tag, p);
			break;
		}
		case Twrite:
			nf = findfid(get32(b), 0);
			if(nf < 0){ rerror(tag, "unknown fid"); break; }
			if(fidnode[nf] != 1){ rerror(tag, "read-only file"); break; }
			count = get32(b + 12);
			if(count > sizeof sbuf - 1)
				count = sizeof sbuf - 1;
			memcpy(sbuf, b + 16, count);
			sbuf[count] = 0;
			e = ctl(sbuf);
			if(e != nil){ rerror(tag, e); break; }
			p = put32(p, count);
			sendreply(type + 1, tag, p);
			break;
		case Tclunk:
		case Tremove:
			nf = findfid(get32(b), 0);
			if(nf >= 0)
				fids[nf] = -1;
			if(type == Tremove){ rerror(tag, "not removable"); break; }
			sendreply(type + 1, tag, p);
			break;
		case Tstat: {
			uchar *sz;
			char *nm;

			nf = findfid(get32(b), 0);
			if(nf < 0){ rerror(tag, "unknown fid"); break; }
			node = fidnode[nf];
			nm = node == 0 ? "/" : node == 1 ? "ctl" :
				(node & 1) ? "status" : svcs[(node - 10) / 2].name;
			sz = p;
			p = put16(p, 0);
			p = pstat(p, nm, node);
			put16(sz, p - sz - 2);
			sendreply(type + 1, tag, p);
			break;
		}
		default:
			rerror(tag, "svc: unsupported request");
		}
	}
	return 0;
}
