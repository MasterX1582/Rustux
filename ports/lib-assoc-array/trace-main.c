#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
uint64_t wrap_assoc_seq(uint64_t, int64_t, int64_t);
int main(int c, char **v)
{
	if (c < 4)
		return 2;
	printf("%016llx\n", (unsigned long long)wrap_assoc_seq(
		(uint64_t)strtoull(v[1], 0, 0), strtoll(v[2], 0, 0),
		strtoll(v[3], 0, 0)));
	return 0;
}
