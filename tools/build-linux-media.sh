#!/bin/sh
# Pinned modern NVENC runtime for the Linux AppImage. No NVIDIA driver is bundled.
set -eu
media_prefix=${1:?Supply an absolute installation prefix}
case "$media_prefix" in /*) ;; *) echo 'Prefix must be absolute' >&2; exit 1;; esac
media_source=${2:?Supply an absolute source/build directory}
case "$media_source" in /*) ;; *) echo 'Source directory must be absolute' >&2; exit 1;; esac
mkdir -p "$media_source"
cd "$media_source"
for project in gstreamer gst-plugins-base gst-plugins-bad; do
    archive="$project-1.26.10.tar.xz"
    if [ ! -f "$archive" ]; then curl --proto '=https' --tlsv1.2 -fsSL "https://gstreamer.freedesktop.org/src/$project/$archive" -o "$archive"; fi
    case "$project" in
        gstreamer) expected=d7f20bec75edeb8677662926c33e987da64a42616c24fc3353b9ad44ed750cd6;;
        gst-plugins-base) expected=1c1531dd8f2d480c89c57b08a930545a3375077391789762e40e490cdbbf03fd;;
        gst-plugins-bad) expected=fec973dff512b507d9dcb5a828e04e061e52188f4d5989e953aed6a41beda437;;
    esac
    printf '%s  %s\n' "$expected" "$archive" | sha256sum -c -
    if [ ! -d "$project-1.26.10" ]; then tar -xf "$archive"; fi
done
meson setup core-build gstreamer-1.26.10 --prefix="$media_prefix" --libdir=lib --buildtype=release -Dauto_features=disabled -Dtests=disabled -Dbenchmarks=disabled -Dexamples=disabled -Dtools=enabled -Dintrospection=disabled
ninja -C core-build -j4
ninja -C core-build install
export PKG_CONFIG_PATH="$media_prefix/lib/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
export LD_LIBRARY_PATH="$media_prefix/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
meson setup base-build gst-plugins-base-1.26.10 --prefix="$media_prefix" --libdir=lib --buildtype=release -Dauto_features=disabled -Dtests=disabled -Dexamples=disabled -Dintrospection=disabled
ninja -C base-build -j4
ninja -C base-build install
meson setup bad-build gst-plugins-bad-1.26.10 --prefix="$media_prefix" --libdir=lib --buildtype=release -Dauto_features=disabled -Dtests=disabled -Dexamples=disabled -Dintrospection=disabled -Dnvcodec=enabled
ninja -C bad-build -j4
ninja -C bad-build install
