/* pkg: the ipnx package manager, v1 (design.md 2026-08-29).
 *
 * A package is a subtree under /pkg/<name>/<version>; installing BINDS it —
 * the union directory is the merge mechanism, the namespace is the
 * installation record (fork shares the namespace group, so a bind made here
 * lands in the invoking shell). No database: /pkg/.installed lists installs,
 * each package keeps a .digests manifest, and every fetched byte is
 * sha256-verified against the registry's published digest — refusal on
 * mismatch, as pip already practises.
 *
 * Registries are trees of files, local or over '#H' (webfs):
 *   <base>/index          lines: name version kind url sha256
 *     kind bin:  url is one wasm; lands at <pkgroot>/bin/<name>
 *     kind tree: url is a manifest — lines: relpath url sha256
 *   <pkgroot>/meta        optional; lines: bind [-a|-b|-c] src dst
 *                         (src relative to the package root); default when
 *                         absent: bind -a <pkgroot>/bin /bin
 * Relative urls join the registry base, so one index serves a same-origin
 * mirror, a local tree (the suite's offline registry), or absolute URLs.
 *
 *   pkg install name[@version]     pkg list
 *   pkg remove name                pkg verify name
 *   pkg -r <base> ...              use one registry instead of
 *                                  /lib/pkg/registries
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

static char *
urlpath(char *url)			/* '#H/<hex-of-url>' for the webfs walk */
{
	static char p[LINELEN*2 + 4];
	int i, n = strlen(url);

	p[0] = '#'; p[1] = 'H'; p[2] = '/';
	for(i = 0; i < n; i++){
		p[3+i*2]   = "0123456789abcdef"[(uchar)url[i] >> 4];
		p[3+i*2+1] = "0123456789abcdef"[(uchar)url[i] & 15];
	}
	p[3+n*2] = 0;
	return p;
}

static int
fetchopen(char *base, char *url)	/* local trees and http, one opener */
{
	char full[LINELEN];

	if(strncmp(url, "http://", 7) == 0 || strncmp(url, "https://", 8) == 0)
		return open(urlpath(url), OREAD);
	if(url[0] == '/')
		return open(url, OREAD);
	snprint(full, sizeof full, "%s/%s", base, url);
	if(full[0] == '/')
		return open(full, OREAD);
	return open(urlpath(full), OREAD);
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
	int fd, n, i = 0;

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

/* ---- bookkeeping: /pkg/.installed — "name version" per line ---- */

static void
recordinstall(char *name, char *ver)
{
	char line[LINELEN];
	int fd;

	mkdirs("/pkg/.installed");
	fd = open("/pkg/.installed", ORDWR);
	if(fd < 0)
		fd = create("/pkg/.installed", ORDWR, 0644);
	if(fd < 0)
		return;
	while(getline9(fd, line, sizeof line) >= 0)
		;
	fprint(fd, "%s %s\n", name, ver);
	close(fd);
}

static int
findinstall(char *name, char *ver, int max, int removeit)
{
	static char keep[64][LINELEN];
	char line[LINELEN], *f[4];
	int fd, n = 0, found = 0, i;

	fd = open("/pkg/.installed", OREAD);
	if(fd < 0)
		return 0;
	while(getline9(fd, line, sizeof line) >= 0 && n < 64){
		strecpy(keep[n], keep[n] + LINELEN, line);
		if(fields(line, f, 4) >= 2 && strcmp(f[0], name) == 0){
			if(ver)
				strecpy(ver, ver + max, f[1]);
			found = 1;
			if(removeit)
				continue;
		}
		n++;
		if(!found || removeit)
			continue;
	}
	close(fd);
	if(removeit && found){
		fd = create("/pkg/.installed", OWRITE, 0644);
		for(i = 0; i < n; i++){
			char probe[LINELEN], *g[4];
			strecpy(probe, probe + LINELEN, keep[i]);
			if(fields(probe, g, 4) >= 2 && strcmp(g[0], name) == 0)
				continue;
			fprint(fd, "%s\n", keep[i]);
		}
		close(fd);
	}
	return found;
}

/* ---- meta: bind lines, or the default bin bind ---- */

static void
applymeta(char *root)
{
	char path[LINELEN], line[LINELEN], src[LINELEN], *f[6];
	int fd, n, flag, bound = 0;

	snprint(path, sizeof path, "%s/meta", root);
	fd = open(path, OREAD);
	if(fd >= 0){
		while(getline9(fd, line, sizeof line) >= 0){
			if(line[0] == '#')
				continue;
			n = fields(line, f, 6);
			if(n < 3 || strcmp(f[0], "bind") != 0)
				continue;
			flag = 0;
			if(n == 4){
				if(strcmp(f[1], "-a") == 0) flag = MAFTER;
				else if(strcmp(f[1], "-b") == 0) flag = MBEFORE;
				f[1] = f[2]; f[2] = f[3];
			}
			if(f[1][0] == '.')
				snprint(src, sizeof src, "%s%s", root, f[1]+1);
			else
				strecpy(src, src + sizeof src, f[1]);
			if(bind(src, f[2], flag) < 0)
				fprint(2, "pkg: bind %s %s failed: %r\n", src, f[2]);
			else{
				print("pkg: bound %s onto %s\n", src, f[2]);
				bound++;
			}
		}
		close(fd);
	}
	if(!bound){
		snprint(src, sizeof src, "%s/bin", root);
		fd = open(src, OREAD);
		if(fd >= 0){
			close(fd);
			if(bind(src, "/bin", MAFTER) < 0)
				fprint(2, "pkg: bind %s /bin failed: %r\n", src);
			else
				print("pkg: bound %s onto /bin\n", src);
		}
	}
}

/* ---- install ---- */

static void
fetchone(char *base, char *url, char *dst, char *want, int fdig)
{
	char got[65];
	int sfd;
	vlong n;

	sfd = fetchopen(base, url);
	if(sfd < 0){
		fprint(2, "pkg: cannot fetch %s: %r\n", url);
		exits("fetch");
	}
	n = sink(sfd, dst, got);
	close(sfd);
	if(cistrcmp(got, want) != 0){
		fprint(2, "pkg: sha256 MISMATCH for %s\n  want %s\n  got  %s\n", url, want, got);
		remove(dst);
		exits("digest");
	}
	print("pkg:   %s (%lld bytes, sha256 verified)\n", dst, n);
	if(fdig >= 0)
		fprint(fdig, "%s %s\n", got, dst);
}

static void
install(char *spec)
{
	char *name, *wantver = nil;
	char base[LINELEN], line[LINELEN], root[LINELEN], dst[LINELEN], dig[LINELEN], v[64];
	char *f[8];
	int i, fd, n, fdig;

	name = strdup(spec);
	{ char *at = strchr(name, '@'); if(at){ *at = 0; wantver = at + 1; } }
	if(findinstall(name, v, sizeof v, 0)){
		print("pkg: %s %s is already installed\n", name, v);
		return;
	}
	for(i = 0; eachbase(i, base, sizeof base) == 0; i++){
		fd = fetchopen(base, "index");
		if(fd < 0)
			continue;
		while(getline9(fd, line, sizeof line) >= 0){
			if(line[0] == '#')
				continue;
			n = fields(line, f, 8);
			if(n < 5 || strcmp(f[0], name) != 0)
				continue;
			if(wantver && strcmp(f[1], wantver) != 0)
				continue;
			print("pkg: installing %s %s from %s\n", name, f[1], base);
			snprint(root, sizeof root, "/pkg/%s/%s", name, f[1]);
			snprint(dig, sizeof dig, "%s/.digests", root);
			mkdirs(dig);
			fdig = create(dig, OWRITE, 0644);
			if(strcmp(f[2], "bin") == 0){
				snprint(dst, sizeof dst, "%s/bin/%s", root, name);
				fetchone(base, f[3], dst, f[4], fdig);
			} else if(strcmp(f[2], "tree") == 0){
				char mpath[LINELEN], mline[LINELEN], *mf[4];
				int mfd, sfd2;
				char got[65];
				snprint(mpath, sizeof mpath, "%s/.manifest", root);
				sfd2 = fetchopen(base, f[3]);
				if(sfd2 < 0){ fprint(2, "pkg: no manifest %s: %r\n", f[3]); exits("fetch"); }
				sink(sfd2, mpath, got);
				close(sfd2);
				if(cistrcmp(got, f[4]) != 0){
					fprint(2, "pkg: manifest sha256 MISMATCH for %s\n", name);
					exits("digest");
				}
				mfd = open(mpath, OREAD);
				while(getline9(mfd, mline, sizeof mline) >= 0){
					if(mline[0] == '#' || fields(mline, mf, 4) < 3)
						continue;
					snprint(dst, sizeof dst, "%s/%s", root, mf[0]);
					fetchone(base, mf[1], dst, mf[2], fdig);
				}
				close(mfd);
			} else {
				fprint(2, "pkg: unknown kind '%s'\n", f[2]);
				exits("kind");
			}
			if(fdig >= 0)
				close(fdig);
			close(fd);
			recordinstall(name, f[1]);
			applymeta(root);
			print("pkg: installed %s %s\n", name, f[1]);
			return;
		}
		close(fd);
	}
	fprint(2, "pkg: '%s' not found in any registry\n", spec);
	exits("notfound");
}

/* ---- list / verify / remove ---- */

static void
list(void)
{
	char line[LINELEN];
	int fd;

	fd = open("/pkg/.installed", OREAD);
	if(fd < 0)
		return;
	while(getline9(fd, line, sizeof line) >= 0)
		print("%s\n", line);
	close(fd);
}

static void
verify(char *name)
{
	char v[64], dig[LINELEN], line[LINELEN], got[65], *f[4];
	uchar buf[CHUNK], sum[32];
	SHA256state st;
	int fd, dfd, n, bad = 0;

	if(!findinstall(name, v, sizeof v, 0)){
		fprint(2, "pkg: %s is not installed\n", name);
		exits("notfound");
	}
	snprint(dig, sizeof dig, "/pkg/%s/%s/.digests", name, v);
	fd = open(dig, OREAD);
	if(fd < 0){
		fprint(2, "pkg: %s has no digest manifest\n", name);
		exits("nodigests");
	}
	while(getline9(fd, line, sizeof line) >= 0){
		if(fields(line, f, 4) < 2)
			continue;
		dfd = open(f[1], OREAD);
		if(dfd < 0){ print("pkg: MISSING %s\n", f[1]); bad++; continue; }
		sha256init(&st);
		while((n = read(dfd, buf, sizeof buf)) > 0)
			sha256update(&st, buf, n);
		close(dfd);
		sha256final(&st, sum);
		hex(sum, 32, got);
		if(cistrcmp(got, f[0]) != 0){ print("pkg: ALTERED %s\n", f[1]); bad++; }
	}
	close(fd);
	if(bad)
		exits("verify");
	print("pkg: %s %s verifies clean\n", name, v);
}

static void
removepkg(char *name)
{
	char v[64], dig[LINELEN], line[LINELEN], src[LINELEN], *f[4];
	int fd;

	if(!findinstall(name, v, sizeof v, 0)){
		fprint(2, "pkg: %s is not installed\n", name);
		exits("notfound");
	}
	snprint(src, sizeof src, "/pkg/%s/%s/bin", name, v);
	unmount(src, "/bin");	/* best effort; meta binds fall away with the tree */
	snprint(dig, sizeof dig, "/pkg/%s/%s/.digests", name, v);
	fd = open(dig, OREAD);
	if(fd >= 0){
		while(getline9(fd, line, sizeof line) >= 0)
			if(fields(line, f, 4) >= 2)
				remove(f[1]);
		close(fd);
	}
	remove(dig);
	snprint(line, sizeof line, "/pkg/%s/%s/.manifest", name, v);
	remove(line);
	snprint(line, sizeof line, "/pkg/%s/%s/meta", name, v);
	remove(line);
	findinstall(name, nil, 0, 1);
	print("pkg: removed %s %s (binds fall away; empty dirs remain until reboot)\n", name, v);
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
		fprint(2, "usage: pkg [-r base] install name[@ver] | list | remove name | verify name\n");
		exits("usage");
	}
	if(strcmp(argv[i], "install") == 0 && i+1 < argc)
		install(argv[i+1]);
	else if(strcmp(argv[i], "list") == 0)
		list();
	else if(strcmp(argv[i], "verify") == 0 && i+1 < argc)
		verify(argv[i+1]);
	else if(strcmp(argv[i], "remove") == 0 && i+1 < argc)
		removepkg(argv[i+1]);
	else{
		fprint(2, "usage: pkg [-r base] install name[@ver] | list | remove name | verify name\n");
		exits("usage");
	}
	exits(nil);
	return 0;
}
