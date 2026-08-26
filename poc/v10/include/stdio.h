/* libv10: the slice of V10's stdio its commands ask for, rewritten over the
 * kernel interface (poc/v10/lib/stdio.c). Unix calls arrive by K&R implicit
 * declaration, as they did then. */
typedef struct FILE FILE;
extern FILE *stdin, *stdout, *stderr;

int	putchar(int);
int	fprintf(FILE*, char*, ...);
void	perror(char*);
int	fflush(FILE*);
#define NULL 0
