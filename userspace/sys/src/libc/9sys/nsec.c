#include <u.h>
#include <libc.h>

#define	U32(x)	(((((((x)[0]<<8)|(x)[1])<<8)|(x)[2])<<8)|(x)[3])

vlong
nsec(void)
{
	uchar b[8];
	int f, n;

	if((f = open("/dev/bintime", OREAD)) >= 0){
		n = pread(f, b, sizeof(b), 0);
		close(f);
		/*
		 * kencc promotes uchar to UNSIGNED int (cc/sub.c:688-691
		 * steps TUCHAR up through TUSHORT to TUINT); clang, like
		 * ANSI C, to int. So U32 is signed here, and the low word
		 * sign-extended into the high one whenever its bit 31 was
		 * set: date -n printed 0 or -1 half the time. The cast
		 * restores what kencc compiled.
		 */
		if(n == sizeof(b))
			return (u64int)U32(b)<<32 | (u32int)U32(b+4);
	}
	return 0;
}
