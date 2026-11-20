#!/bin/bash
# scratch coverage check (not part of the package)
cd /tmp
P="$(cd "$(dirname "$0")" && pwd)"
for s in 42 1 3 1000 deadbeef; do
  cat > aat_$s.c <<EOC
#include <stdio.h>
#include <stdint.h>
uint64_t wrap_assoc_seq(uint64_t seed, int64_t nops_i, int64_t seed2);
int main(void){ printf("%016llx\n",(unsigned long long)wrap_assoc_seq(0x$s,149,0)); return 0; }
EOC
  gcc -O2 -w -I$P -o aat_$s aat_$s.c $P/wrappers.c $P/assoc_array.c || exit 1
  KTRACE=1 ./aat_$s 2> /tmp/aa_$s.trace > /tmp/aa_$s.out
  echo "seed $s: checksum $(cat /tmp/aa_$s.out) sc-folds $(grep -c '^F 00000000002' /tmp/aa_$s.trace) frees $(grep -c fee1 /tmp/aa_$s.trace) folds $(wc -l < /tmp/aa_$s.trace)"
done
