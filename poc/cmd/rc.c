/* rc: a minimal Plan 9 shell.
 *
 * The subset (documented in poc/README.md): words with '...' quoting and #
 * comments; $var and x=v assignments (lists arrive via `{...}); `{...} command
 * substitution; * ? globbing in the final path component; pipelines |;
 * ; & && ||; redirections > >> <; if(list) pline, if not pline,
 * for(x in words) pline — single-pipeline bodies, no braces; builtins
 * cd, ~, exit; subshells (...) — which run in a bare-forked copy of the
 * interpreter, the asyncify path. Functions are still refused.
 *
 * Every fork is procrfork(RFFDG, fn, arg): the child dups, closes and
 * execs inside the fork guard's extent (RESEARCH.md §5.2).
 */
#include "lib9.h"

typedef struct Word Word;
struct Word { char *s; Word *next; };
typedef struct Redir Redir;
struct Redir { int type; char *raw; Redir *next; };	/* type: '>', 'a'(>>), '<' */
typedef struct Cmd Cmd;
typedef struct Pline Pline;
typedef struct Seq Seq;
struct Cmd {
	int type;		/* 'x' simple, 'i' if, 'n' if not, 'f' for, 's' subshell */
	Word *raw;		/* simple: raw words (unexpanded) */
	Redir *redirs;
	Seq *cond;		/* if */
	char *forvar;
	Word *forraw;		/* for: raw list */
	Pline *body;		/* if/for */
	Seq *sub;		/* subshell */
};
struct Pline { Cmd *stage[16]; int n; };
typedef struct AndOr AndOr;
struct AndOr { Pline *p; int op; AndOr *next; };	/* op after p: 0 end, 'a' &&, 'o' || */
struct Seq { AndOr *ao; int bg; Seq *next; };

typedef struct Var Var;
struct Var { char *name; Word *val; Var *next; };

static Var *vars;
static char status[128];
static int lastif = -1;		/* 1 true, 0 false, -1 none */
static int interactive;

/* ---------- tokens ---------- */
enum { TEOF, TWORD, TSEMI, TAMP, TPIPE, TANDAND, TOROR, TGT, TGTGT, TLT, TLP, TRP };
typedef struct Tok Tok;
struct Tok { int type; char *s; };
static Tok toks[256];
static int ntok, tpos;
static char *perr;

static int
special(int c)
{
	return c==' '||c=='\t'||c==';'||c=='&'||c=='|'||c=='>'||c=='<'||
	       c=='('||c==')'||c=='#'||c==0;
}

static int
lex(char *s)
{
	char wbuf[1024];
	int wn;

	ntok = 0; tpos = 0; perr = nil;
	for(;;){
		while(*s==' ' || *s=='\t')
			s++;
		if(*s==0 || *s=='#')
			break;
		if(ntok >= 255){ perr = "too many tokens"; return -1; }
		switch(*s){
		case ';': toks[ntok++].type = TSEMI; s++; continue;
		case '(': toks[ntok++].type = TLP; s++; continue;
		case ')': toks[ntok++].type = TRP; s++; continue;
		case '<': toks[ntok++].type = TLT; s++; continue;
		case '>':
			if(s[1]=='>'){ toks[ntok++].type = TGTGT; s += 2; }
			else { toks[ntok++].type = TGT; s++; }
			continue;
		case '&':
			if(s[1]=='&'){ toks[ntok++].type = TANDAND; s += 2; }
			else { toks[ntok++].type = TAMP; s++; }
			continue;
		case '|':
			if(s[1]=='|'){ toks[ntok++].type = TOROR; s += 2; }
			else { toks[ntok++].type = TPIPE; s++; }
			continue;
		}
		/* a word: copy raw text, tracking quotes and `{...} */
		wn = 0;
		while(!special(*s & 0xff) || 0){
			if(wn > 1000){ perr = "word too long"; return -1; }
			if(*s=='\''){
				wbuf[wn++] = *s++;
				for(;;){
					if(*s==0){ perr = "unterminated quote"; return -1; }
					if(*s=='\'' && s[1]=='\''){ wbuf[wn++]='\''; wbuf[wn++]='\''; s += 2; continue; }
					if(*s=='\''){ wbuf[wn++] = *s++; break; }
					wbuf[wn++] = *s++;
				}
				continue;
			}
			if(*s=='`' && s[1]=='{'){
				int depth = 1;
				wbuf[wn++] = *s++;
				wbuf[wn++] = *s++;
				while(depth > 0){
					if(*s==0){ perr = "unterminated `{"; return -1; }
					if(*s=='{') depth++;
					if(*s=='}') depth--;
					wbuf[wn++] = *s++;
				}
				continue;
			}
			wbuf[wn++] = *s++;
		}
		wbuf[wn] = 0;
		toks[ntok].type = TWORD;
		toks[ntok].s = strdup(wbuf);
		ntok++;
	}
	toks[ntok].type = TEOF;
	return 0;
}

static Tok *peek(void){ return &toks[tpos]; }
static Tok *next(void){ return &toks[tpos++]; }

/* ---------- parser ---------- */
static Seq *parseseq(int stopatrp);
static Pline *parsepline(void);

static Cmd *
parsecmd(void)
{
	Cmd *c = malloc(sizeof *c);
	Word **wtail;
	Redir **rtail;

	memset(c, 0, sizeof *c);
	c->type = 'x';
	if(peek()->type==TLP){
		next();
		c->type = 's';
		c->sub = parseseq(1);
		if(c->sub == nil){
			if(perr == nil)
				perr = "empty subshell";
			return nil;
		}
		if(next()->type != TRP){ perr = "expected ) after subshell"; return nil; }
		rtail = &c->redirs;
		while(peek()->type==TGT || peek()->type==TGTGT || peek()->type==TLT){
			Redir *r = malloc(sizeof *r);
			int t = next()->type;
			r->type = t==TGT ? '>' : t==TGTGT ? 'a' : '<';
			if(peek()->type != TWORD){ perr = "expected file after redirection"; return nil; }
			r->raw = next()->s;
			r->next = nil;
			*rtail = r; rtail = &r->next;
		}
		return c;
	}
	if(peek()->type==TWORD && strcmp(peek()->s, "if")==0){
		next();
		if(peek()->type==TWORD && strcmp(peek()->s, "not")==0){
			next();
			c->type = 'n';
			c->body = parsepline();
			return c->body ? c : nil;
		}
		if(next()->type != TLP){ perr = "expected ( after if"; return nil; }
		c->type = 'i';
		c->cond = parseseq(1);
		if(c->cond == nil)
			return nil;
		if(next()->type != TRP){ perr = "expected ) after if condition"; return nil; }
		c->body = parsepline();
		return c->body ? c : nil;
	}
	if(peek()->type==TWORD && strcmp(peek()->s, "for")==0){
		next();
		if(next()->type != TLP){ perr = "expected ( after for"; return nil; }
		if(peek()->type != TWORD){ perr = "expected variable after for("; return nil; }
		c->type = 'f';
		c->forvar = next()->s;
		if(peek()->type != TWORD || strcmp(peek()->s, "in") != 0){
			perr = "expected 'in' in for(...)";
			return nil;
		}
		next();
		wtail = &c->forraw;
		while(peek()->type == TWORD){
			Word *w = malloc(sizeof *w);
			w->s = next()->s;
			w->next = nil;
			*wtail = w; wtail = &w->next;
		}
		if(next()->type != TRP){ perr = "expected ) after for list"; return nil; }
		c->body = parsepline();
		return c->body ? c : nil;
	}
	wtail = &c->raw;
	rtail = &c->redirs;
	for(;;){
		int t = peek()->type;
		if(t==TWORD){
			Word *w = malloc(sizeof *w);
			w->s = next()->s;
			w->next = nil;
			*wtail = w; wtail = &w->next;
			continue;
		}
		if(t==TGT || t==TGTGT || t==TLT){
			Redir *r = malloc(sizeof *r);
			r->type = t==TGT ? '>' : t==TGTGT ? 'a' : '<';
			next();
			if(peek()->type != TWORD){ perr = "expected file after redirection"; return nil; }
			r->raw = next()->s;
			r->next = nil;
			*rtail = r; rtail = &r->next;
			continue;
		}
		break;
	}
	if(c->raw == nil && c->redirs == nil){
		perr = "expected command";
		return nil;
	}
	return c;
}

static Pline *
parsepline(void)
{
	Pline *p = malloc(sizeof *p);

	p->n = 0;
	for(;;){
		Cmd *c = parsecmd();
		if(c == nil)
			return nil;
		if(p->n >= 16){ perr = "pipeline too long"; return nil; }
		p->stage[p->n++] = c;
		if(peek()->type != TPIPE)
			break;
		next();
	}
	return p;
}

static Seq *
parseseq(int stopatrp)
{
	Seq *head = nil, **stail = &head;

	for(;;){
		Seq *sq;
		AndOr *ao, **atail;

		if(peek()->type==TEOF || (stopatrp && peek()->type==TRP))
			break;
		sq = malloc(sizeof *sq);
		sq->bg = 0; sq->next = nil; sq->ao = nil;
		atail = &sq->ao;
		for(;;){
			ao = malloc(sizeof *ao);
			ao->p = parsepline();
			ao->op = 0; ao->next = nil;
			if(ao->p == nil)
				return nil;
			*atail = ao; atail = &ao->next;
			if(peek()->type==TANDAND){ next(); ao->op = 'a'; continue; }
			if(peek()->type==TOROR){ next(); ao->op = 'o'; continue; }
			break;
		}
		if(peek()->type==TAMP){ next(); sq->bg = 1; }
		else if(peek()->type==TSEMI)
			next();
		*stail = sq; stail = &sq->next;
	}
	return head ? head : (Seq*)0;
}

/* ---------- variables, status ---------- */
static Word *
mkword(char *s, Word *next)
{
	Word *w = malloc(sizeof *w);
	w->s = s; w->next = next;
	return w;
}

static void
setvar(char *name, Word *val)
{
	Var *v;

	for(v = vars; v; v = v->next)
		if(strcmp(v->name, name)==0){ v->val = val; return; }
	v = malloc(sizeof *v);
	v->name = strdup(name); v->val = val; v->next = vars;
	vars = v;
}

static Word *
getvar(char *name)
{
	Var *v;

	if(strcmp(name, "status")==0)
		return status[0] ? mkword(strdup(status), nil) : mkword("", nil);
	for(v = vars; v; v = v->next)
		if(strcmp(v->name, name)==0)
			return v->val;
	return nil;
}

/* ---------- await bookkeeping ---------- */
typedef struct Dead Dead;
struct Dead { int pid; char *msg; Dead *next; };
static Dead *deadlist;

static char *
awaitpid(int pid)
{
	char buf[192], *q, *e;
	Dead *d, **dp;
	int got;

	for(dp = &deadlist; (d = *dp); dp = &d->next)
		if(d->pid == pid){ *dp = d->next; return d->msg; }
	for(;;){
		if(await(buf, sizeof buf) < 0)
			return "await failed";
		got = atoi(buf);
		q = strchr(buf, '\'');
		e = q ? strchr(q+1, 0) : nil;
		if(q && e > q+1 && e[-1]=='\'')
			e[-1] = 0;
		q = q ? strdup(q+1) : "";
		if(got == pid)
			return q;
		d = malloc(sizeof *d);
		d->pid = got; d->msg = q; d->next = deadlist;
		deadlist = d;
	}
}

/* ---------- glob ---------- */
static int
match(char *s, char *p)
{
	for(; *p; p++, s++){
		if(*p=='*'){
			for(p++; ; s++){
				if(match(s, p))
					return 1;
				if(*s==0)
					return 0;
			}
		}
		if(*p=='?' && *s)
			continue;
		if(*p != *s)
			return 0;
	}
	return *s==0;
}

static Word *
globword(char *w)
{
	char dir[512], name[128], *base, *slash;
	uchar buf[4096], *p;
	Word *out = nil, **tail = &out;
	int fd, n, sz;

	slash = nil;
	for(p = (uchar*)w; *p; p++)
		if(*p=='/')
			slash = (char*)p;
	if(slash){
		n = slash - w;
		if(n == 0){ dir[0]='/'; dir[1]=0; }
		else { memcpy(dir, w, n); dir[n] = 0; }
		base = slash+1;
	} else {
		strcpy(dir, ".");
		base = w;
	}
	fd = open(dir, OREAD);
	if(fd < 0)
		return mkword(w, nil);
	while((n = read(fd, buf, sizeof buf)) > 0)
		for(p = buf; p < buf+n; p += sz){
			sz = (p[0] | p[1]<<8) + 2;
			statname(p, name, sizeof name);
			if(match(name, base)){
				char *full = malloc(strlen(dir)+strlen(name)+2);
				if(slash){
					strcpy(full, dir);
					full[strlen(dir)] = '/';
					strcpy(full+strlen(dir)+1, name);
				} else
					strcpy(full, name);
				*tail = mkword(full, nil);
				tail = &(*tail)->next;
			}
		}
	close(fd);
	return out ? out : mkword(w, nil);	/* no match: word unchanged, per rc */
}

/* ---------- expansion ---------- */
static char *runcapture(char*, Word**);

static Word *
segjoin(Word *acc, Word *seg)
{
	Word *w, *o, *out = nil, **tail = &out;
	int i, n, na = 0, ns = 0;

	if(acc == nil || seg == nil)
		return acc ? acc : seg;
	for(w = acc; w; w = w->next) na++;
	for(w = seg; w; w = w->next) ns++;
	if(na != 1 && ns != 1 && na != ns){
		fprint(2, "rc: bad concatenation (%d and %d elements)\n", na, ns);
		return acc;
	}
	n = na > ns ? na : ns;
	w = acc; o = seg;
	for(i = 0; i < n; i++){
		char *j = malloc(strlen(w->s)+strlen(o->s)+1);
		strcpy(j, w->s);
		strcpy(j+strlen(w->s), o->s);
		*tail = mkword(j, nil);
		tail = &(*tail)->next;
		if(na != 1) w = w->next;
		if(ns != 1) o = o->next;
	}
	return out;
}

static Word *
expandone(char *raw, int doglob)
{
	Word *acc = nil, *seg;
	char sbuf[1024], nbuf[64];
	int sn, quoted = 0, anyseg = 0;
	char *s = raw;

	while(*s){
		seg = nil;
		if(*s=='\''){
			sn = 0; s++;
			for(;;){
				if(*s=='\'' && s[1]=='\''){ sbuf[sn++]='\''; s += 2; continue; }
				if(*s=='\'' || *s==0){ if(*s) s++; break; }
				sbuf[sn++] = *s++;
			}
			sbuf[sn] = 0;
			seg = mkword(strdup(sbuf), nil);
			quoted = 1;
		} else if(*s=='`' && s[1]=='{'){
			int depth = 1;
			sn = 0; s += 2;
			while(depth > 0 && *s){
				if(*s=='{') depth++;
				if(*s=='}'){ depth--; if(depth==0){ s++; break; } }
				sbuf[sn++] = *s++;
			}
			sbuf[sn] = 0;
			runcapture(sbuf, &seg);
		} else if(*s=='$'){
			int nn = 0;
			s++;
			while((*s>='a'&&*s<='z')||(*s>='A'&&*s<='Z')||(*s>='0'&&*s<='9')||*s=='_')
				nbuf[nn++] = *s++;
			nbuf[nn] = 0;
			seg = getvar(nbuf);
			if(seg == nil)
				continue;	/* unset: empty list */
		} else {
			sn = 0;
			while(*s && *s!='\'' && *s!='$' && !(*s=='`' && s[1]=='{'))
				sbuf[sn++] = *s++;
			sbuf[sn] = 0;
			seg = mkword(strdup(sbuf), nil);
		}
		acc = anyseg ? segjoin(acc, seg) : seg;
		anyseg = 1;
	}
	if(acc && doglob && !quoted){
		Word *out = nil, **tail = &out, *w, *g;
		for(w = acc; w; w = w->next){
			if(strchr(w->s, '*') || strchr(w->s, '?')){
				for(g = globword(w->s); g; g = g->next){
					*tail = mkword(g->s, nil);
					tail = &(*tail)->next;
				}
			} else {
				*tail = mkword(w->s, nil);
				tail = &(*tail)->next;
			}
		}
		return out;
	}
	return acc;
}

static Word *
expandlist(Word *raw, int doglob)
{
	Word *out = nil, **tail = &out, *r, *e;

	for(r = raw; r; r = r->next)
		for(e = expandone(r->s, doglob); e; e = e->next){
			*tail = mkword(e->s, nil);
			tail = &(*tail)->next;
		}
	return out;
}

/* ---------- execution ---------- */
static void runseq(Seq*);
static void runpline(Pline*, int bg);

typedef struct Ex Ex;
struct Ex {
	char *argv[64];
	int dups[8][2];		/* dup from -> to */
	int ndup;
	int closefds[40];
	int nclose;
};

static void
childexec(void *v)
{
	Ex *e = v;
	char path[512];
	int i;

	for(i = 0; i < e->ndup; i++)
		dup(e->dups[i][0], e->dups[i][1]);
	for(i = 0; i < e->nclose; i++)
		close(e->closefds[i]);
	if(strchr(e->argv[0], '/'))
		exec(e->argv[0], e->argv);
	else {
		strcpy(path, "/bin/");
		strcpy(path+5, e->argv[0]);
		exec(path, e->argv);
	}
	fprint(2, "rc: %s: %r\n", e->argv[0]);
	exits("exec");
}

static int
isassign(char *raw, char **eq)
{
	char *p = raw;

	if(!((*p>='a'&&*p<='z')||(*p>='A'&&*p<='Z')||*p=='_'))
		return 0;
	while((*p>='a'&&*p<='z')||(*p>='A'&&*p<='Z')||(*p>='0'&&*p<='9')||*p=='_')
		p++;
	if(*p != '=')
		return 0;
	*eq = p;
	return 1;
}

/* returns raw words with leading assignments consumed (applied) */
static Word *
doassigns(Cmd *c, Word *raw)
{
	char *eq, name[64];

	while(raw && isassign(raw->s, &eq)){
		int n = eq - raw->s;
		memcpy(name, raw->s, n);
		name[n] = 0;
		setvar(name, expandlist(mkword(eq+1, nil), 1));
		raw = raw->next;
	}
	USED(c);
	return raw;
}

static int
builtin(Word *argv)
{
	Word *w;

	if(strcmp(argv->s, "cd")==0){
		char *dst = argv->next ? argv->next->s : "/";
		if(chdir(dst) < 0){
			fprint(2, "rc: cd %s: %r\n", dst);
			strcpy(status, "cd");
		} else
			status[0] = 0;
		return 1;
	}
	if(strcmp(argv->s, "~")==0){
		Word *subj = argv->next, *pat;
		if(subj == nil){ strcpy(status, "no match"); return 1; }
		for(pat = subj->next; pat; pat = pat->next)
			if(match(subj->s, pat->s)){
				status[0] = 0;
				return 1;
			}
		strcpy(status, "no match");
		return 1;
	}
	if(strcmp(argv->s, "exit")==0){
		char *m = argv->next ? argv->next->s : status;
		exits(m[0] ? m : nil);
	}
	USED(w);
	return 0;
}

static void
runsimple(Cmd *c, int infd, int outfd, int *closefds, int nclose, int *pidout)
{
	Ex *e = malloc(sizeof *e);
	Word *argv, *w;
	Redir *r;
	int i, n = 0;

	*pidout = -1;
	argv = doassigns(c, c->raw);
	if(argv == nil && c->redirs == nil){
		status[0] = 0;	/* assignments only */
		return;
	}
	argv = expandlist(argv, 1);
	if(argv == nil){
		fprint(2, "rc: null command\n");
		strcpy(status, "null");
		return;
	}
	if(infd < 0 && outfd < 0 && c->redirs == nil && builtin(argv))
		return;
	e->ndup = 0; e->nclose = 0;
	if(infd >= 0){
		e->dups[e->ndup][0] = infd; e->dups[e->ndup][1] = 0; e->ndup++;
	}
	if(outfd >= 0){
		e->dups[e->ndup][0] = outfd; e->dups[e->ndup][1] = 1; e->ndup++;
	}
	for(i = 0; i < nclose; i++)
		e->closefds[e->nclose++] = closefds[i];
	for(r = c->redirs; r; r = r->next){
		Word *f = expandlist(mkword(r->raw, nil), 1);
		int fd;
		if(f == nil || f->next){
			fprint(2, "rc: redirection needs one file\n");
			strcpy(status, "redir");
			return;
		}
		if(r->type == '<')
			fd = open(f->s, OREAD);
		else if(r->type == 'a'){
			fd = open(f->s, OWRITE);
			if(fd >= 0)
				seek(fd, 0, 2);
			else
				fd = create(f->s, OWRITE, 0644);
		} else
			fd = create(f->s, OWRITE, 0644);
		if(fd < 0){
			fprint(2, "rc: can't open %s: %r\n", f->s);
			strcpy(status, "redir");
			return;
		}
		e->dups[e->ndup][0] = fd;
		e->dups[e->ndup][1] = r->type=='<' ? 0 : 1;
		e->ndup++;
		e->closefds[e->nclose++] = fd;
	}
	for(w = argv, n = 0; w && n < 63; w = w->next)
		e->argv[n++] = w->s;
	e->argv[n] = nil;
	for(i = 0; i < e->ndup; i++)
		e->closefds[e->nclose++] = e->dups[i][0];
	*pidout = procrfork(RFFDG, childexec, e);
	/* parent closes its copies of this stage's redir fds */
	for(r = c->redirs, i = e->ndup - 1; r; r = r->next, i--)
		close(e->dups[i][0]);
	if(*pidout < 0){
		fprint(2, "rc: fork: %r\n");
		strcpy(status, "fork");
	}
}

static void
subfork(Cmd *c, int infd, int outfd, int *closefds, int nclose, int *pidout)
{
	Redir *r;
	int i, pid;

	pid = rfork(RFFDG|RFPROC);	/* bare: both sides keep interpreting */
	if(pid < 0){
		fprint(2, "rc: fork: %r\n");
		strcpy(status, "fork");
		*pidout = -1;
		return;
	}
	if(pid == 0){
		if(infd >= 0) dup(infd, 0);
		if(outfd >= 0) dup(outfd, 1);
		for(i = 0; i < nclose; i++)
			close(closefds[i]);
		for(r = c->redirs; r; r = r->next){
			Word *f = expandlist(mkword(r->raw, nil), 1);
			int fd;
			if(f == nil || f->next)
				exits("redir");
			if(r->type == '<')
				fd = open(f->s, OREAD);
			else if(r->type == 'a'){
				fd = open(f->s, OWRITE);
				if(fd >= 0) seek(fd, 0, 2);
				else fd = create(f->s, OWRITE, 0644);
			} else
				fd = create(f->s, OWRITE, 0644);
			if(fd < 0){
				fprint(2, "rc: can't open %s: %r\n", f->s);
				exits("redir");
			}
			dup(fd, r->type=='<' ? 0 : 1);
			close(fd);
		}
		runseq(c->sub);
		exits(status[0] ? status : nil);
	}
	*pidout = pid;
}

static void
runpline(Pline *p, int bg)
{
	int pipes[15][2], pids[16], closefds[40];
	int i, nclose = 0;

	if(p->n == 1 && p->stage[0]->type != 'x' && p->stage[0]->type != 's'){
		Cmd *c = p->stage[0];
		if(c->type == 'i'){
			runseq(c->cond);
			lastif = status[0]==0;
			if(lastif)
				runpline(c->body, 0);
			return;
		}
		if(c->type == 'n'){
			if(lastif == -1){
				fprint(2, "rc: if not without if\n");
				strcpy(status, "if");
				return;
			}
			if(!lastif)
				runpline(c->body, 0);
			lastif = -1;
			return;
		}
		if(c->type == 'f'){
			Word *w;
			for(w = expandlist(c->forraw, 1); w; w = w->next){
				setvar(c->forvar, mkword(w->s, nil));
				runpline(c->body, 0);
			}
			return;
		}
	}
	for(i = 0; i < p->n; i++)
		if(p->stage[i]->type != 'x' && p->stage[i]->type != 's'){
			fprint(2, "rc: if/for in a pipeline: wrap it in ( )\n");
			strcpy(status, "pipeline");
			return;
		}
	if(p->n == 1){
		if(p->stage[0]->type == 's')
			subfork(p->stage[0], -1, -1, nil, 0, &pids[0]);
		else
			runsimple(p->stage[0], -1, -1, nil, 0, &pids[0]);
		if(pids[0] > 0){
			if(bg){ status[0] = 0; return; }
			strcpy(status, awaitpid(pids[0]));
		}
		return;
	}
	for(i = 0; i < p->n - 1; i++){
		if(pipe(pipes[i]) < 0){
			fprint(2, "rc: pipe: %r\n");
			strcpy(status, "pipe");
			return;
		}
		closefds[nclose++] = pipes[i][0];
		closefds[nclose++] = pipes[i][1];
	}
	for(i = 0; i < p->n; i++){
		int infd = i > 0 ? pipes[i-1][0] : -1;
		int outfd = i < p->n - 1 ? pipes[i][1] : -1;
		if(p->stage[i]->type == 's')
			subfork(p->stage[i], infd, outfd, closefds, nclose, &pids[i]);
		else
			runsimple(p->stage[i], infd, outfd, closefds, nclose, &pids[i]);
	}
	for(i = 0; i < nclose; i++)
		close(closefds[i]);
	if(bg){ status[0] = 0; return; }
	for(i = 0; i < p->n; i++)
		if(pids[i] > 0){
			char *m = awaitpid(pids[i]);
			if(i == p->n - 1)
				strcpy(status, m);
		}
}

static void
runseq(Seq *sq)
{
	AndOr *ao;

	for(; sq; sq = sq->next)
		for(ao = sq->ao; ao; ao = ao->next){
			runpline(ao->p, sq->bg && ao->next==nil);
			if(ao->op=='a' && status[0]!=0)
				break;
			if(ao->op=='o' && status[0]==0)
				break;
		}
}

static void
runline(char *line)
{
	Seq *sq;

	if(lex(line) < 0){
		fprint(2, "rc: %s\n", perr);
		strcpy(status, "syntax");
		return;
	}
	if(ntok == 0)
		return;
	sq = parseseq(0);
	if(sq == nil || peek()->type != TEOF){
		fprint(2, "rc: %s\n", perr ? perr : "syntax error");
		strcpy(status, "syntax");
		return;
	}
	runseq(sq);
}

/* run text in a child rc, capture fd 1, split on whitespace */
typedef struct Cap Cap;
struct Cap { char *text; int wr; };
static void
capchild(void *v)
{
	Cap *cp = v;
	char *av[4];

	dup(cp->wr, 1);
	close(cp->wr);
	av[0] = "rc"; av[1] = "-c"; av[2] = cp->text; av[3] = nil;
	exec("/bin/rc", av);
	fprint(2, "rc: can't exec /bin/rc: %r\n");
	exits("exec");
}

static char *
runcapture(char *text, Word **out)
{
	Cap *cp = malloc(sizeof *cp);
	char *buf = malloc(16384), wbuf[1024];
	Word **tail = out;
	int fd[2], pid, n, tot = 0, wn = 0;

	*out = nil;
	if(pipe(fd) < 0)
		return "pipe";
	cp->text = strdup(text);
	cp->wr = fd[1];
	pid = procrfork(RFFDG, capchild, cp);
	close(fd[1]);
	if(pid < 0){ close(fd[0]); return "fork"; }
	while((n = read(fd[0], buf+tot, 16383-tot)) > 0)
		tot += n;
	close(fd[0]);
	strcpy(status, awaitpid(pid));
	for(n = 0; n <= tot; n++){
		if(n==tot || buf[n]==' ' || buf[n]=='\t' || buf[n]=='\n'){
			if(wn){
				wbuf[wn] = 0;
				*tail = mkword(strdup(wbuf), nil);
				tail = &(*tail)->next;
				wn = 0;
			}
		} else
			wbuf[wn++] = buf[n];
	}
	return nil;
}

/* ---------- main ---------- */
static int
getline(int fd, char *buf, int n)
{
	int i = 0;
	char c;

	while(i < n-1){
		if(read(fd, &c, 1) <= 0)
			return i ? i : -1;
		if(c == '\n')
			break;
		buf[i++] = c;
	}
	buf[i] = 0;
	return i;
}

static void
runtext(char *text)
{
	char line[1024];
	int i;

	while(*text){
		for(i = 0; *text && *text != '\n' && i < 1023; )
			line[i++] = *text++;
		if(*text == '\n')
			text++;
		line[i] = 0;
		runline(line);
	}
}

int
main(int argc, char *argv[])
{
	char line[1024];
	static char script[32768];
	int fd, n, tot;

	status[0] = 0;
	if(argc >= 3 && strcmp(argv[1], "-c")==0){
		runtext(argv[2]);
		exits(status[0] ? status : nil);
	}
	if(argc >= 2){
		fd = open(argv[1], OREAD);
		if(fd < 0){
			fprint(2, "rc: can't open %s: %r\n", argv[1]);
			exits("open");
		}
		tot = 0;
		while((n = read(fd, script+tot, sizeof script - 1 - tot)) > 0)
			tot += n;
		close(fd);
		script[tot] = 0;
		runtext(script);
		exits(status[0] ? status : nil);
	}
	interactive = 1;
	for(;;){
		write(1, "% ", 2);
		if(getline(0, line, sizeof line) < 0)
			break;
		runline(line);
	}
	write(1, "\n", 1);
	exits(status[0] ? status : nil);
	return 0;
}
