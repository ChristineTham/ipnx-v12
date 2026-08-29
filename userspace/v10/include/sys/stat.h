/* V10 personality: struct stat over the kernel's stat(5) records */
struct stat {
	int	st_dev;
	long	st_ino;
	int	st_mode;
	int	st_nlink;
	int	st_uid;
	int	st_gid;
	long	st_size;
};
#define S_IFMT	0170000
#define S_IFDIR	0040000
#define S_IFCHR	0020000
#define S_IFBLK	0060000
#define S_IFREG	0100000

int stat(char*, struct stat*);
int fstat(int, struct stat*);
