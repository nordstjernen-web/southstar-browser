#!/usr/bin/env bash
# Build a Southstar AppImage. Uses linuxdeploy + its GTK plugin to
# bundle the GTK 4 stack (libs, typelibs, pixbuf loaders, GIO modules)
# so the AppImage runs on distros without modern GTK preinstalled.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
VERSION=$(grep -E "^[[:space:]]*version" "$ROOT/meson.build" | head -1 \
          | sed -E "s/.*version: '([^']+)'.*/\1/")
ARCH=$(uname -m)
NAME=southstar
BUILDDIR="$ROOT/build-appimage"
APPDIR="$ROOT/dist/AppDir"
TOOLS="$ROOT/dist/appimage-tools"

mkdir -p "$ROOT/dist" "$TOOLS"

LINUXDEPLOY="$TOOLS/linuxdeploy-${ARCH}.AppImage"
GTKPLUGIN="$TOOLS/linuxdeploy-plugin-gtk.sh"
APPIMAGETOOL="$TOOLS/appimagetool-${ARCH}.AppImage"
RUNTIME="$TOOLS/runtime-${ARCH}"

LINUXDEPLOY_RELEASE=1-alpha-20251107-1
APPIMAGETOOL_RELEASE=1.9.1
RUNTIME_RELEASE=20251108
GTKPLUGIN_COMMIT=7a3fbc31a9e5075073ff8790f26effbac5f84453
GTKPLUGIN_SHA256=b0f4cbc684a0103a9651f0955b635eaea0096b3a66c0f5a2c2aa337960375171
case "$ARCH" in
    x86_64)
        LINUXDEPLOY_SHA256=c20cd71e3a4e3b80c3483cef793cda3f4e990aca14014d23c544ca3ce1270b4d
        APPIMAGETOOL_SHA256=ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0
        RUNTIME_SHA256=2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d
        ;;
    aarch64)
        LINUXDEPLOY_SHA256=620095110d693282b8ebeb244a95b5e911cf8f65f76c88b4b47d16ae6346fcff
        APPIMAGETOOL_SHA256=f0837e7448a0c1e4e650a93bb3e85802546e60654ef287576f46c71c126a9158
        RUNTIME_SHA256=00cbdfcf917cc6c0ff6d3347d59e0ca1f7f45a6df1a428a0d6d8a78664d87444
        ;;
    *)
        echo "pack-appimage: no pinned AppImage tools for $ARCH" >&2
        exit 1
        ;;
esac

fetch_tool() {
    local path=$1 url=$2 sha256=$3
    if [ -f "$path" ] && echo "$sha256  $path" | sha256sum --check --status; then
        return
    fi
    echo "Fetching $(basename "$path")..."
    rm -f "$path"
    curl -L --fail -o "$path.part" "$url"
    if ! echo "$sha256  $path.part" | sha256sum --check --status; then
        rm -f "$path.part"
        echo "pack-appimage: checksum mismatch for $url" >&2
        exit 1
    fi
    mv "$path.part" "$path"
    chmod +x "$path"
}

fetch_tool "$LINUXDEPLOY" \
    "https://github.com/linuxdeploy/linuxdeploy/releases/download/$LINUXDEPLOY_RELEASE/linuxdeploy-${ARCH}.AppImage" \
    "$LINUXDEPLOY_SHA256"
fetch_tool "$GTKPLUGIN" \
    "https://raw.githubusercontent.com/linuxdeploy/linuxdeploy-plugin-gtk/$GTKPLUGIN_COMMIT/linuxdeploy-plugin-gtk.sh" \
    "$GTKPLUGIN_SHA256"
fetch_tool "$APPIMAGETOOL" \
    "https://github.com/AppImage/appimagetool/releases/download/$APPIMAGETOOL_RELEASE/appimagetool-${ARCH}.AppImage" \
    "$APPIMAGETOOL_SHA256"
fetch_tool "$RUNTIME" \
    "https://github.com/AppImage/type2-runtime/releases/download/$RUNTIME_RELEASE/runtime-${ARCH}" \
    "$RUNTIME_SHA256"

if [ ! -d "$BUILDDIR" ]; then
    meson setup "$BUILDDIR" \
        --prefix=/usr \
        --buildtype=release \
        -Db_lto=true \
        -Db_ndebug=true \
        --strip
fi
meson compile -C "$BUILDDIR"

rm -rf "$APPDIR"
DESTDIR="$APPDIR" meson install -C "$BUILDDIR"

rm -rf "$APPDIR/usr/include" "$APPDIR/usr/lib" "$APPDIR/usr/lib64" \
       "$APPDIR/usr/lib32"

install -dm755 "$APPDIR/usr/share/applications"
install -m644 "$ROOT/data/southstar.desktop" \
    "$APPDIR/usr/share/applications/org.southstar.WebBrowser.desktop"

install -dm755 "$APPDIR/usr/share/icons/hicolor/256x256/apps"
ICON_SVG="$ROOT/data/icons/hicolor/scalable/apps/southstar.svg"
ICON_GIF="$ROOT/data/icons/hicolor/scalable/apps/southstar.gif"
PNG_ICON="$APPDIR/usr/share/icons/hicolor/256x256/apps/southstar.png"
if command -v rsvg-convert >/dev/null 2>&1; then
    rsvg-convert -w 256 -h 256 -o "$PNG_ICON" "$ICON_SVG"
elif command -v magick >/dev/null 2>&1; then
    magick "$ICON_GIF[0]" -resize 256x256 "$PNG_ICON"
elif command -v convert >/dev/null 2>&1; then
    convert "$ICON_GIF[0]" -resize 256x256 "$PNG_ICON"
else
    echo "Need rsvg-convert or ImageMagick to rasterise the icon." >&2
    exit 1
fi

cd "$ROOT/dist"
rm -f "${NAME}-${VERSION}-${ARCH}.AppImage" "Southstar"*.AppImage
DEPLOY_GTK_VERSION=4 \
"$LINUXDEPLOY" --appimage-extract-and-run \
    --appdir "$APPDIR" \
    --executable "$APPDIR/usr/bin/southstar" \
    --executable "$APPDIR/usr/bin/southstar-renderer" \
    --desktop-file "$APPDIR/usr/share/applications/org.southstar.WebBrowser.desktop" \
    --icon-file "$PNG_ICON" \
    --plugin gtk

FINAL="$ROOT/dist/${NAME}-${VERSION}-${ARCH}.AppImage"
ARCH="${ARCH}" "$APPIMAGETOOL" --appimage-extract-and-run \
    --runtime-file "$RUNTIME" \
    --no-appstream \
    "$APPDIR" "$FINAL"

chmod +x "$FINAL"

echo
echo "Built: $FINAL ($(du -h "$FINAL" | cut -f1))"
echo
echo "Run:    $FINAL https://example.com"
echo "Smoke:  $FINAL --headless --url=https://example.com --dump=text"
