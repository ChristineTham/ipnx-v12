/* pkg: the ipnx package manager, v2 (docs/type.md, accepted 2026-09-02).
 *
 * A PACKAGE IS A DECLARATION, not a directory of bytes. `/pkg/<name>` is a
 * file — "a list of bindings plus commands" — and the bytes it names live in
 * `/store/<name>/<version>`, fetched once, verified against a digest pinned in
 * the declaration, and BOUND. So:
 *
 *   install   materialise into the store if absent, verify, then bind
 *   remove    an UNBIND. The store entry survives, so reinstalling is free
 *             and rollback costs nothing
 *   list      `ls /pkg`. The declarations ARE the record — no database
 *
 * A declaration, and `cat /pkg/python` is the whole audit:
 *
 *   fetch  <src>  <sha256>  /store/python/3.14/bin/python
 *   bind   [-a|-b] /store/python/3.14/bin  /bin
 *   env    PYTHONHOME /store/python/3.14
 *
 * The three verbs are type.md's. The little language is /lib/namespace's —
 * one format across /profile, /pkg and /template (decision log 2026-09-02),
 * so nothing new had to be designed.
 *
 * Immutability is not enforced here: `/store` is served by storefs, which
 * refuses to rewrite an existing entry. pkg pins and checks the digest; the
 * store's server keeps the bytes honest afterwards. Both are IPNX programs,
 * which is the point of the 2026-09-04 decision.
 *
 * Registries are trees of files, LOCAL ONLY for now: '#H' left the kernel
 * (P1 step 5) and the userspace webfs that replaces it is not written, so an
 * http(s) base is refused with an error that says so. `<base>/<name>` is the
 * declaration; a relative `fetch` source joins the base.
 *
 *   pkg install name               pkg list
 *   pkg remove name                pkg verify name
 *   pkg -r <base> ...              use one registry instead of
 *                                  /lib/pkg/registries
 *
 * GAP, raised rather than invented: a package whose content is a TREE. Python's
 * stdlib is 539 files, and type.md names `fetch` for a file only. Declaring 539
 * fetch lines is not an audit anyone reads. The shape that keeps the property —
 * one pinned digest covering a manifest that pins the rest — is not in the spec,
 * so it is not built here.
 */
#include "lib9.h"

enum { LINELEN = 1024, CHUNK = 16384 };

static char *forcebase;

/* ---- small utilities (2MB pooled guest memory: stream, never slurp) ---- */

static void
hex(uchar *d, int n, char *out)
{
	int i;

	for(i = 0; i < n; i++){
		out[i*2]   = "0123456789abcdef"[d[i] >> 4];
		out[i*2+1] = "0123456789abcdef"[d[i] & 15];
	}
	out[n*2] = 0;
}

static int
fetchopen(char *base, char *url)	/* local trees and http, one opener */
{
	char full[LINELEN];

	if(strncmp(url, "http://", 7) == 0 || strncmp(url, "https://", 8) == 0){
		/* '#H' left the kernel in P1 step 5: fetching is not the kernel's
		 * job, and Plan 9 answers this with a USERSPACE webfs
		 * (plan9/sys/src/cmd/webfs). Until one is here, a registry is a
		 * local tree. */
		fprint(2, "pkg: %s: http registries need a userspace webfs "
		          "('#H' left the kernel); use a local path\n", url);
		return -1;
	}
	if(url[0] == '/')
		return open(url, OREAD);
	snprint(full, sizeof full, "%s/%s", base, url);
	if(full[0] == '/')
		return open(full, OREAD);
	fprint(2, "pkg: %s: a relative url on a non-local base needs a "
	          "userspace webfs\n", full);
	return -1;
}

static void
mkdirs(char *path)			/* create each missing component */
{
	char buf[LINELEN], *p;
	int fd;

	strecpy(buf, buf + sizeof buf, path);
	for(p = buf + 1; *p; p++){
		if(*p != '/')
			continue;
		*p = 0;
		fd = open(buf, OREAD);
		if(fd < 0){
			fd = create(buf, OREAD, DMDIR | 0755);
			if(fd < 0){
				fprint(2, "pkg: cannot create %s: %r\n", buf);
				exits("mkdir");
			}
		}
		close(fd);
		*p = '/';
	}
}

/* hex sha256 of an existing file; 0 on success */
static int
hashfile(char *path, char *digest)
{
	SHA256state st;
	uchar buf[CHUNK], sum[32];
	int fd, n;

	fd = open(path, OREAD);
	if(fd < 0)
		return -1;
	sha256init(&st);
	while((n = read(fd, buf, sizeof buf)) > 0)
		sha256update(&st, buf, n);
	close(fd);
	sha256final(&st, sum);
	hex(sum, 32, digest);
	return 0;
}

/* stream src fd into dst path, hashing; returns bytes, hex in digest */
static vlong
sink(int sfd, char *dst, char *digest)
{
	SHA256state st;
	uchar buf[CHUNK], sum[32];
	int dfd, n;
	vlong total = 0;

	mkdirs(dst);
	dfd = create(dst, OWRITE, 0755);
	if(dfd < 0){
		fprint(2, "pkg: cannot create %s: %r\n", dst);
		exits("create");
	}
	sha256init(&st);
	while((n = read(sfd, buf, sizeof buf)) > 0){
		sha256update(&st, buf, n);
		if(write(dfd, buf, n) != n){
			fprint(2, "pkg: short write on %s\n", dst);
			exits("write");
		}
		total += n;
	}
	close(dfd);
	sha256final(&st, sum);
	hex(sum, 32, digest);
	return total;
}

static int
getline9(int fd, char *line, int max)	/* one line, byte at a time (index files are small) */
{
	int i = 0, n;
	char c;

	for(;;){
		n = read(fd, &c, 1);
		if(n <= 0)
			return i > 0 ? i : -1;
		if(c == '\n')
			break;
		if(i < max - 1)
			line[i++] = c;
	}
	line[i] = 0;
	return i;
}

/* a stat record's own length: size[2], little-endian, plus the two bytes it
 * does not count. lib9's stat accessors read fields, not the frame. */
static uint
reclen(uchar *e)
{
	return (e[0] | (e[1] << 8)) + 2;
}

static int
fields(char *line, char **f, int max)
{
	int n = 0;

	while(*line && n < max){
		while(*line == ' ' || *line == '\t')
			*line++ = 0;
		if(*line == 0)
			break;
		f[n++] = line;
		while(*line && *line != ' ' && *line != '\t')
			line++;
	}
	return n;
}


/* ---- the registry list ---- */

static int
eachbase(int idx, char *base, int max)	/* idx'th registry base, or -1 */
{
	char line[LINELEN], *f[4];
	int fd, i = 0;

	if(forcebase){
		if(idx > 0)
			return -1;
		strecpy(base, base + max, forcebase);
		return 0;
	}
	fd = open("/lib/pkg/registries", OREAD);
	if(fd < 0){
		fprint(2, "pkg: no /lib/pkg/registries (and no -r)\n");
		exits("registries");
	}
	while(getline9(fd, line, sizeof line) >= 0){
		if(line[0] == '#' || fields(line, f, 4) < 2)
			continue;
		if(i++ == idx){
			strecpy(base, base + max, f[1]);
			close(fd);
			return 0;
		}
	}
	close(fd);
	return -1;
}

/* ---- declarations ---- */

static char *
decl(char *name)			/* /pkg/<name> */
{
	static char p[LINELEN];

	snprint(p, sizeof p, "/pkg/%s", name);
	return p;
}

static int
opensource(char *base, char *src, char *full, int max)
{
	if(strncmp(src, "http://", 7) == 0 || strncmp(src, "https://", 8) == 0){
		fprint(2, "pkg: %s: http sources need a userspace webfs "
		          "('#H' left the kernel); use a local path\n", src);
		exits("nofetch");
	}
	if(src[0] == '/')
		strecpy(full, full + max, src);
	else
		snprint(full, max, "%s/%s", base, src);
	return open(full, OREAD);
}

/* fetch one file into the store, unless it is already there. The digest is
 * checked either way: a store entry that no longer hashes to what the
 * declaration pinned is a false claim, and saying so is the whole point. */
static void
dofetch(char *base, char *src, char *want, char *dst)
{
	char full[LINELEN], got[65];
	int sfd, fd;
	vlong n;

	fd = open(dst, OREAD);
	if(fd >= 0){
		close(fd);
		if(hashfile(dst, got) == 0 && cistrcmp(got, want) == 0)
			return;			/* already in the store, and honest */
		fprint(2, "pkg: %s: store entry does not match the pinned digest\n", dst);
		exits("digest");
	}
	sfd = opensource(base, src, full, sizeof full);
	if(sfd < 0){
		fprint(2, "pkg: %s: %r\n", full);
		exits("fetch");
	}
	n = sink(sfd, dst, got);
	close(sfd);
	if(cistrcmp(got, want) != 0){
		fprint(2, "pkg: %s: sha256 mismatch\n\twant %s\n\tgot  %s\n", full, want, got);
		remove(dst);
		exits("digest");
	}
	print("pkg: fetched %s (%lld bytes) into %s\n", src, n, dst);
}

/* "a name that would bind over different bytes is refused" (design 2026-08-29):
 * nothing global to corrupt, and nothing silently shadowed by accident. */
static void
conflictcheck(char *src, char *dst)
{
	uchar edir[512];
	char name[128], a[LINELEN], b[LINELEN], da[65], db[65];
	int fd, n;

	fd = open(src, OREAD);
	if(fd < 0)
		return;
	while((n = read(fd, edir, sizeof edir)) > 0){
		uchar *e = edir, *end = edir + n;

		while(e < end){
			uint rec = reclen(e);

			if(rec < 2 || e + rec > end)
				break;
			statname(e, name, sizeof name);
			snprint(a, sizeof a, "%s/%s", src, name);
			snprint(b, sizeof b, "%s/%s", dst, name);
			if(hashfile(b, db) == 0 && hashfile(a, da) == 0
			&& cistrcmp(da, db) != 0){
				fprint(2, "pkg: CONFLICT: %s already resolves to different bytes\n", b);
				close(fd);
				exits("conflict");
			}
			e += rec;
		}
	}
	close(fd);
}

/* run a declaration's lines. `act` 0 = install (fetch, bind, env),
 * 1 = verify (check digests only), 2 = remove (unbind only). */
static void
apply(char *declpath, char *base, int act)
{
	char line[LINELEN], *f[6], envp[LINELEN];
	int fd, n, flag, bad = 0;

	fd = open(declpath, OREAD);
	if(fd < 0){
		fprint(2, "pkg: %s: %r\n", declpath);
		exits("notfound");
	}
	while(getline9(fd, line, sizeof line) >= 0){
		if(line[0] == '#' || line[0] == 0)
			continue;
		n = fields(line, f, 6);
		if(n < 3)
			continue;
		if(strcmp(f[0], "fetch") == 0){
			if(act == 0)
				dofetch(base, f[1], f[2], f[3]);
			else if(act == 1){
				char got[65];

				if(hashfile(f[3], got) != 0){
					print("pkg: MISSING %s\n", f[3]);
					bad++;
				} else if(cistrcmp(got, f[2]) != 0){
					print("pkg: ALTERED %s\n", f[3]);
					bad++;
				}
			}
		} else if(strcmp(f[0], "bind") == 0){
			flag = 0;
			if(f[1][0] == '-'){
				if(strcmp(f[1], "-a") == 0) flag = MAFTER;
				else if(strcmp(f[1], "-b") == 0) flag = MBEFORE;
				f[1] = f[2]; f[2] = f[3];
			}
			if(act == 0){
				conflictcheck(f[1], f[2]);
				if(bind(f[1], f[2], flag) < 0)
					fprint(2, "pkg: bind %s %s: %r\n", f[1], f[2]);
			} else if(act == 2){
				if(unmount(f[1], f[2]) < 0)
					fprint(2, "pkg: unmount %s %s: %r\n", f[1], f[2]);
			}
		} else if(strcmp(f[0], "env") == 0 && act == 0){
			int efd;

			snprint(envp, sizeof envp, "/env/%s", f[1]);
			efd = create(envp, OWRITE, 0644);
			if(efd >= 0){
				write(efd, f[2], strlen(f[2]));
				close(efd);
			}
		}
	}
	close(fd);
	if(act == 1){
		if(bad)
			exits("verify");
		print("pkg: %s verifies clean\n", declpath);
	}
}

/* ---- install ---- */

static void
install(char *name)
{
	char base[LINELEN], from[LINELEN], dig[65];
	int i, sfd, fd;

	fd = open(decl(name), OREAD);
	if(fd >= 0){
		close(fd);
		print("pkg: %s is already installed\n", name);
		return;
	}
	for(i = 0; eachbase(i, base, sizeof base) == 0; i++){
		snprint(from, sizeof from, "%s/%s", base, name);
		sfd = open(from, OREAD);
		if(sfd < 0)
			continue;
		close(sfd);
		/* Apply from the REGISTRY's copy, and record only once it has
		 * worked: a refused install must leave nothing behind, or
		 * `pkg list` lies about what is installed. */
		apply(from, base, 0);
		mkdirs(decl(name));
		sfd = open(from, OREAD);
		if(sfd < 0){
			fprint(2, "pkg: %s: %r\n", from);
			exits("record");
		}
		sink(sfd, decl(name), dig);	/* the declaration IS the record */
		close(sfd);
		print("pkg: installed %s\n", name);
		return;
	}
	fprint(2, "pkg: '%s' not found in any registry\n", name);
	exits("notfound");
}

/* ---- list / verify / remove ---- */

static void
list(void)
{
	uchar edir[512];
	char name[128];
	int fd, n;

	fd = open("/pkg", OREAD);
	if(fd < 0)
		return;
	while((n = read(fd, edir, sizeof edir)) > 0){
		uchar *e = edir, *end = edir + n;

		while(e < end){
			uint rec = reclen(e);

			if(rec < 2 || e + rec > end)
				break;
			print("%s\n", statname(e, name, sizeof name));
			e += rec;
		}
	}
	close(fd);
}

static void
removepkg(char *name)
{
	apply(decl(name), "", 2);		/* unbind; the store entry survives */
	if(remove(decl(name)) < 0){
		fprint(2, "pkg: %s: %r\n", decl(name));
		exits("remove");
	}
	print("pkg: removed %s (an unbind; the store entry stays)\n", name);
}

int
main(int argc, char *argv[])
{
	int i = 1;

	if(i < argc && strcmp(argv[i], "-r") == 0 && i+1 < argc){
		forcebase = argv[i+1];
		i += 2;
	}
	if(i >= argc){
		fprint(2, "usage: pkg [-r base] install name | list | remove name | verify name\n");
		exits("usage");
	}
	if(strcmp(argv[i], "install") == 0 && i+1 < argc)
		install(argv[i+1]);
	else if(strcmp(argv[i], "list") == 0)
		list();
	else if(strcmp(argv[i], "verify") == 0 && i+1 < argc)
		apply(decl(argv[i+1]), "", 1);
	else if(strcmp(argv[i], "remove") == 0 && i+1 < argc)
		removepkg(argv[i+1]);
	else{
		fprint(2, "usage: pkg [-r base] install name | list | remove name | verify name\n");
		exits("usage");
	}
	exits(nil);
	return 0;
}
