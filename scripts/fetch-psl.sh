#!/usr/bin/env bash
# Put the Public Suffix List into a bundle's data directory (DEST/public_suffix_list.dat),
# where rust/psl reads it at runtime: the system's copy when the build machine has one,
# otherwise a download from publicsuffix.org. The list is MPL-2.0 data shipped as a
# separate file; nothing of it is compiled into Southstar.
set -euo pipefail

DEST=${1:?usage: fetch-psl.sh DEST_DIR}
URL=${NS_PSL_URL:-https://publicsuffix.org/list/public_suffix_list.dat}
mkdir -p "$DEST"

for dir in /usr/share/publicsuffix /usr/local/share/publicsuffix /opt/homebrew/share/publicsuffix; do
    if [ -s "$dir/public_suffix_list.dat" ]; then
        cp "$dir/public_suffix_list.dat" "$DEST/public_suffix_list.dat"
        exit 0
    fi
done

curl -fsSL --retry 3 "$URL" -o "$DEST/public_suffix_list.dat.part"
grep -q '^// ===BEGIN ICANN DOMAINS===' "$DEST/public_suffix_list.dat.part"
mv "$DEST/public_suffix_list.dat.part" "$DEST/public_suffix_list.dat"
