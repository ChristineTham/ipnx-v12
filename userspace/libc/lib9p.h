/* lib9p: the guest side of wire 9P2000 — marshal vocabulary and the server
 * loop's plumbing, shared by every file server (hellofs, exportfs). */

enum {
	Tversion = 100, Tauth = 102, Tattach = 104, Terror9 = 106,
	Twalk = 110, Topen = 112, Tcreate = 114, Tread = 116, Twrite = 118,
	Tclunk = 120, Tremove = 122, Tstat = 124, Twstat = 126,
	Rerror9 = 107,
	MSIZE9 = 8216,
	QTDIR9 = 0x80, QTFILE9 = 0x00,
};

uint    get16(uchar *p);
uint    get32(uchar *p);
uvlong  get64(uchar *p);
uchar*  put8(uchar *p, uint v);
uchar*  put16(uchar *p, uint v);
uchar*  put32(uchar *p, uint v);
uchar*  put64(uchar *p, uvlong v);
uchar*  putstr(uchar *p, char *s);
uchar*  putqid(uchar *p, int qtype, uvlong qpath);

long    readn(int fd, void *buf, long n);
/* read one 9P message into buf (>= MSIZE9); returns its length, 0 at EOF,
 * -1 on a framing error */
long    read9msg(int fd, uchar *buf);
/* frame buf[7..end) as (type, tag) and write it to fd */
void    send9msg(int fd, int type, int tag, uchar *buf, uchar *end);
void    send9err(int fd, int tag, char *e, uchar *buf);
