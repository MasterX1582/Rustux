#!/bin/bash
set -e
cd /mnt/c/Users/dev/Documents/GitHub/linux-rust/ports
mkdir -p lib-idr-radix/linux/sched lib-idr-radix/rust
L=/mnt/c/Users/dev/Documents/GitHub/linux
cp "$L/lib/idr.c" lib-idr-radix/idr.c
cp "$L/lib/radix-tree.c" lib-idr-radix/radix-tree.c
cp "$L/lib/radix-tree.h" lib-idr-radix/radix-tree.h
cp lib-xarray/xarray.c lib-idr-radix/xarray.c
cp lib-xarray/linux/xarray.h lib-idr-radix/linux/xarray.h
cp "$L/include/linux/radix-tree.h" lib-idr-radix/linux/radix-tree.h
cp "$L/include/linux/idr.h" lib-idr-radix/linux/idr.h
for f in bitmap.h bug.h compiler.h container_of.h err.h errno.h export.h gfp.h kconfig.h limits.h list.h lockdep.h minmax.h rcupdate.h slab.h spinlock.h stddef.h string.h types.h; do
  cp "lib-xarray/linux/$f" lib-idr-radix/linux/
done
cp lib-xarray/linux/sched/mm.h lib-idr-radix/linux/sched/
ls -la lib-idr-radix lib-idr-radix/linux
