/*
 * `boot` — get a root file system and hand over to init.
 *
 * `plan9/sys/src/9/boot/boot.c`, cut to what this machine needs. There, boot
 * chooses a method (TCP, a local disk, virtio-9p, …), authenticates, and
 * mounts what it got; here there is one method and no authentication, so what
 * is left is `nsinit` (`boot.c:151`) and `execinit` (`:201`).
 *
 * The method is `bootvirtio9p.c`, whose whole body is:
 *
 *	fd = open("#9/0", ORDWR);
 *
 * and whose comment says why there is nothing else to it: *"There is nothing
 * to configure (no network), so connect just opens the device; boot.c then
 * does the 9P version handshake and mounts it as the root."*
 *
 * WHAT IS NOT HERE, and each because there is nothing behind it yet:
 * authentication (`fauth`/`auth_proxy` — no factotum), the swap process, and
 * the partition tables.
 *
 * AND NO PROMPT. Plan 9's `rootserver` (`boot.c:328`) asks `root is from
 * (local, tcp, ...)` and takes its default from `$bootargs`, or skips the
 * question when `$nobootprompt` answers it — which is what plan9.ini is for.
 * With one method there is no question to ask, and so nothing for `$rootspec`
 * or `$rootdir` to answer: `#ec` is attachable and empty.
 */
#include <u.h>
#include <libc.h>

char *rootdir = "/root";

static void
fatal(char *s)
{
	char buf[ERRMAX];

	buf[0] = '\0';
	errstr(buf, sizeof buf);
	fprint(2, "boot: %s: %s\n", s, buf);
	exits(s);
}

/*
 * `srvcreate` (`boot/aux.c:125`) — post the root channel at `#s/boot`, so
 * anything that wants its own namespace can mount the root again without
 * knowing where it came from. That is what `/lib/namespace`'s first line
 * does: `mount -aC #s/boot /root`.
 */
static void
srvcreate(char *name, int fd)
{
	char buf[64];
	int f;

	snprint(buf, sizeof buf, "#s/%s", name);
	f = create(buf, OWRITE, 0666);
	if(f < 0)
		fatal(buf);
	snprint(buf, sizeof buf, "%d", fd);
	if(write(f, buf, strlen(buf)) != strlen(buf))
		fatal("write");
	close(f);
}

/*
 * `authentication` (`boot/bootauth.c:7`) takes its first branch here and
 * always will:
 *
 *	if(access("/boot/factotum", AEXEC) < 0){
 *		glenda();
 *		return;
 *	}
 *
 * There is no factotum in `#/boot` — it carries one file — so this is
 * `glenda()` (`:56`) and nothing else. **It is what names the host owner**:
 * `eve` is the empty string until now (`pc/main.c:285`), every process is
 * nobody, and `hostownerwrite` permits the write because `iseve()` is
 * comparing two empty strings (`auth.c:128`).
 *
 * `$user` comes from `plan9.ini` by way of `#ec`, which exists here and is
 * empty, so the name is the one Plan 9 falls back to.
 */
static void
authentication(void)
{
	char *s;
	int fd;

	s = getenv("user");
	if(s == nil)
		s = "glenda";

	fd = open("#c/hostowner", OWRITE);
	if(fd >= 0){
		if(write(fd, s, strlen(s)) != strlen(s))
			fprint(2, "boot: setting #c/hostowner to %s: %r\n", s);
		close(fd);
	}
}

/*
 * `execinit` (`boot.c:202`), Plan 9's whole: `$init` — a line of
 * `plan9.ini` on a Plan 9 machine, the host's configuration here — is
 * init's command line, tokenized, with its first word's last element as
 * `argv[0]`. With none, it is Plan 9's default, `"/%s/init -%s%s"`: `$cputype`,
 * `t` for a terminal or `c` for a cpu server (`#e/service`, `:256`), and `m`
 * for `boot -m`, which nothing passes here. NOT `/bin/init`, because `/bin` is
 * a union `/lib/namespace` makes and nothing has read that file yet.
 */
static void
execinit(void)
{
	int iargc, cpuflag;
	char *cmd, cmdbuf[64], *iargv[16], *cputype, *service;

	cmd = getenv("init");
	if(cmd == nil){
		cputype = getenv("cputype");
		service = getenv("service");
		cpuflag = service != nil && strcmp(service, "cpu") == 0;
		snprint(cmdbuf, sizeof cmdbuf, "/%s/init -%s%s",
			cputype != nil ? cputype : "", cpuflag ? "c" : "t", "");
		cmd = cmdbuf;
	}
	iargc = tokenize(cmd, iargv, nelem(iargv)-1);
	cmd = iargv[0];

	/* make iargv[0] basename(iargv[0]) */
	if(iargv[0] = strrchr(iargv[0], '/'))
		iargv[0]++;
	else
		iargv[0] = cmd;

	iargv[iargc] = nil;

	exec(cmd, iargv);
	fatal(cmd);
}

void
main(int argc, char *argv[])
{
	int fd;

	USED(argc, argv);

	/*
	 * `nsinit` (`boot.c:151`). The order is the whole of it:
	 *
	 *	bind("/", "/", MREPL)          make the root a union of its own
	 *	mount(fd, afd, "/root", …)     the server, somewhere to stand
	 *	bind(rootdir, "/", MAFTER|MCREATE)
	 *
	 * That last line is what makes **the root a file server**: after it,
	 * `/` answers from `#/` first and from the server after, so `/etc`,
	 * `/rc` and `/lib` are the server's while `/boot` stays the kernel's.
	 */
	if(bind("/", "/", MREPL) < 0)
		fatal("bind /");

	fd = open("#9/0", ORDWR);
	if(fd < 0)
		fatal("open #9/0");

	/*
	 * Plan 9 does `fversion(fd, 0, buf, sizeof buf)` here, before posting
	 * the channel, because `srvcreate` hands the same channel to whoever
	 * mounts it next and the version is negotiated once per connection.
	 * This kernel's `mount` does the handshake (`mntversion`,
	 * `devmnt.c:118`) and refuses `fversion` saying so, so the order is
	 * mount first, post after.
	 */
	if(mount(fd, -1, rootdir, MREPL|MCREATE, "") < 0)
		fatal("mount /root");
	srvcreate("boot", fd);

	if(bind(rootdir, "/", MAFTER|MCREATE) < 0)
		fatal("second bind /");

	authentication();

	execinit();
}
