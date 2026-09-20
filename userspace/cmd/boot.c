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
 * authentication (`fauth`/`auth_proxy` — no factotum), `$rootspec` and
 * `$rootdir` (no `#ec`, so no configuration to read them from), the swap
 * process, and the partition tables.
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

	/*
	 * `execinit` (`boot.c:201`). With no `$init` to read, the name is the
	 * one Plan 9 falls back to: `"/%s/init"` with `$cputype` — NOT
	 * `/bin/init`, because `/bin` is a union `/lib/namespace` makes and
	 * nothing has read that file yet. `#/` carries an empty `bin` for
	 * something to bind onto, and a walk that finds it there stops.
	 */
	exec("/wasm/init", (char*[]){ "init", nil });
	fatal("/wasm/init");
}
