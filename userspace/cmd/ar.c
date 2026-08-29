/* ar: the plain GNU archive, without the ranlib table.
 * Measured (RESEARCH §9.8): wasm-ld scans archive members itself and links an
 * index-less archive, so 'ar r libx.a x.o' followed by 'cc main.c -lx' works
 * with no symbol table at all. Member names longer than 15 characters would
 * need the GNU '//' name table; refused honestly instead. */
#include "lib9.h"

enum { HDR = 60 };
static char magic[] = "!<arch>\n";

typedef struct Member Member;
struct Member {
	char name[17];
	uchar *data;
	long size;
	Member *next;
};

/* the header's numeric fields are fixed-width ASCII; parse without strtol */
static long
field(uchar *p, int w)
{
	long v;
	int i;

	v = 0;
	for(i = 0; i < w && p[i] >= '0' && p[i] <= '9'; i++)
		v = v * 10 + (p[i] - '0');
	return v;
}

static uchar *
readall(int fd, long *np)
{
	uchar *buf;
	long n, sz, got;

	sz = 8192;
	buf = malloc(sz);
	got = 0;
	while((n = read(fd, buf + got, sz - got)) > 0){
		got += n;
		if(got == sz){
			sz *= 2;
			buf = realloc(buf, sz);
		}
	}
	*np = got;
	return buf;
}

static Member *
parsear(uchar *buf, long n, char *path)
{
	Member *head, **tail, *m;
	long off, size;
	int i;

	if(n < 8 || memcmp(buf, magic, 8) != 0){
		fprint(2, "ar: %s is not an archive\n", path);
		exits("format");
	}
	head = nil;
	tail = &head;
	off = 8;
	while(off + HDR <= n){
		m = malloc(sizeof(Member));
		for(i = 0; i < 16 && buf[off+i] != ' '; i++)
			m->name[i] = buf[off+i];
		if(i > 0 && m->name[i-1] == '/')	/* GNU: name ends in '/' */
			i--;
		m->name[i] = 0;
		size = field(buf + off + 48, 10);
		m->size = size;
		m->data = buf + off + HDR;
		m->next = nil;
		*tail = m;
		tail = &m->next;
		off += HDR + size + (size & 1);
	}
	return head;
}

static Member *
loadar(char *path)
{
	int fd;
	long n;
	uchar *buf;

	fd = open(path, OREAD);
	if(fd < 0)
		return nil;
	buf = readall(fd, &n);
	close(fd);
	return parsear(buf, n, path);
}

static void
writear(char *path, Member *list)
{
	int fd, n;
	Member *m;
	char hdr[HDR + 1];

	fd = create(path, OWRITE, 0644);
	if(fd < 0){
		fprint(2, "ar: cannot create %s: %r\n", path);
		exits("create");
	}
	write(fd, magic, 8);
	for(m = list; m != nil; m = m->next){
		/* name/ mtime uid gid mode size `\n — fixed columns, space-padded */
		memset(hdr, ' ', HDR);
		n = strlen(m->name);
		memcpy(hdr, m->name, n);
		hdr[n] = '/';
		hdr[16] = '0';			/* mtime */
		hdr[28] = '0';			/* uid */
		hdr[34] = '0';			/* gid */
		memcpy(hdr + 40, "644", 3);	/* mode */
		snprint(hdr + 48, 12, "%ld", m->size);
		hdr[48 + strlen(hdr + 48)] = ' ';	/* undo snprint's NUL */
		hdr[58] = '`';
		hdr[59] = '\n';
		write(fd, hdr, HDR);
		write(fd, m->data, m->size);
		if(m->size & 1)
			write(fd, "\n", 1);
	}
	close(fd);
}

static char *
basenm(char *p)
{
	char *s;

	s = strrchr(p, '/');
	return s ? s + 1 : p;
}

int
main(int argc, char *argv[])
{
	Member *list, *m, **tail, *nm;
	char *key, *nb;
	int i, fd, hit;
	long n;

	if(argc < 3 || strlen(argv[1]) == 0){
		fprint(2, "usage: ar r|t|x|d archive [file ...]\n");
		exits("usage");
	}
	key = argv[1];
	list = loadar(argv[2]);

	switch(key[0] == 'r' || key[1] == 'r' ? 'r' : key[0]){
	case 'r':
		for(i = 3; i < argc; i++){
			nb = basenm(argv[i]);
			if(strlen(nb) > 15){
				fprint(2, "ar: '%s' exceeds 15 characters (no GNU name table)\n", nb);
				exits("name");
			}
			fd = open(argv[i], OREAD);
			if(fd < 0){
				fprint(2, "ar: cannot open %s: %r\n", argv[i]);
				exits("open");
			}
			nm = malloc(sizeof(Member));
			strcpy(nm->name, nb);
			nm->data = readall(fd, &n);
			nm->size = n;
			nm->next = nil;
			close(fd);
			hit = 0;
			for(m = list; m != nil; m = m->next)
				if(strcmp(m->name, nb) == 0){	/* replace in place */
					m->data = nm->data;
					m->size = nm->size;
					hit = 1;
					break;
				}
			if(!hit){
				for(tail = &list; *tail != nil; tail = &(*tail)->next)
					;
				*tail = nm;
			}
		}
		writear(argv[2], list);
		break;
	case 't':
		for(m = list; m != nil; m = m->next)
			print("%s\n", m->name);
		break;
	case 'x':
		for(m = list; m != nil; m = m->next){
			hit = argc == 3;
			for(i = 3; i < argc; i++)
				if(strcmp(basenm(argv[i]), m->name) == 0)
					hit = 1;
			if(!hit)
				continue;
			fd = create(m->name, OWRITE, 0644);
			if(fd < 0){
				fprint(2, "ar: cannot create %s: %r\n", m->name);
				exits("create");
			}
			write(fd, m->data, m->size);
			close(fd);
		}
		break;
	case 'd':
		for(i = 3; i < argc; i++){
			for(tail = &list; *tail != nil; tail = &(*tail)->next)
				if(strcmp((*tail)->name, basenm(argv[i])) == 0){
					*tail = (*tail)->next;
					break;
				}
		}
		writear(argv[2], list);
		break;
	default:
		fprint(2, "ar: unknown key '%s'\n", key);
		exits("usage");
	}
	exits(0);
}
