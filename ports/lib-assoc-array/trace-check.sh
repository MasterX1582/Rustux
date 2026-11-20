#!/bin/bash
# Differential spot-check: C harness vs Rust port, checksum + fold trace.
cd "$(dirname "$0")"
D=rust/wrappers-rs/target/release/examples/dbg
ok=1
for args in "1 149 0" "7 189 21" "255 149 1" "3735928559 64 2" \
            "305419896 149 -7" "1000 189 255" "3 17 6" "42 149 11" \
            "65535 149 101" "0 149 0" "2 1 0" "18446744073709551615 189 1024"; do
	set -- $args
	c=$(/tmp/tracemain "$1" "$2" "$3")
	r=$($D "$1" "$2" "$3")
	if [ "$c" = "$r" ]; then
		echo "OK   seed=$1 nops=$2 s2=$3  $c"
	else
		echo "DIFF seed=$1 nops=$2 s2=$3  C=$c R=$r"
		ok=0
	fi
done
exit $((1-ok))
