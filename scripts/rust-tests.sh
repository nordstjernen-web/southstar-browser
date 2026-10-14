#!/bin/sh
# Southstar: runs the unit tests of every crate under rust/ whose sources hold
# a #[test], passing any extra arguments on to cargo test.
set -eu

cd "$(dirname "$0")/.."
packages=$(grep -rl --include='*.rs' '#\[test\]' rust | cut -d/ -f2 | sort -u | while read -r dir; do
    printf -- '-p %s ' "$(sed -n 's/^name = "\(.*\)"$/\1/p' "rust/$dir/Cargo.toml" | head -n 1)"
done)
exec cargo test --locked $packages "$@"
