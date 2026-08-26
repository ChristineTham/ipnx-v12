/* libv10 stat: V10's struct stat decoded from the kernel's stat(5) records.
 * st_nlink is served as 1 and character/block devices as regular files until
 * the kernel's stat carries more — recorded as a personality deviation. */
#include "sys/types.h"
#include "sys/stat.h"

extern int _sys(int, int, int, int, int, int);
typedef unsigned char v10uchar;

static long
gle(v10uchar *p, int n)
{
	long v = 0;
	int i;

	for(i = n-1; i >= 0; i--)
		v = (v << 8) | p[i];
	return v;
}

static int
decode(v10uchar *e, int n, struct stat *st)
{
	long mode;

	if(n <= 0)
		return -1;
	mode = gle(e+21, 4);
	st->st_dev = (int)gle(e+4, 2);
	st->st_ino = gle(e+13, 4);		/* qid.path, low half */
	st->st_mode = (mode & 0x80000000 ? S_IFDIR : S_IFREG) | (mode & 0777);
	st->st_nlink = 1;
	st->st_uid = 0;
	st->st_gid = 0;
	st->st_size = gle(e+33, 4);
	return 0;
}

int
stat(char *path, struct stat *st)
{
	v10uchar edir[512];

	return decode(edir, _sys(42, (int)path, (int)edir, sizeof edir, 0, 0), st);
}

int
fstat(int fd, struct stat *st)
{
	v10uchar edir[512];

	return decode(edir, _sys(43, fd, (int)edir, sizeof edir, 0, 0), st);
}
