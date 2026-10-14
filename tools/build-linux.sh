#!/bin/sh
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"
export CC=${CC:-clang-21}
export CXX=${CXX:-clang++-21}
"$CXX" --version >/dev/null
pkg-config --exists gstreamer-1.0 gstreamer-app-1.0 libavcodec libavutil libswscale
cargo build --release --locked --features desktop
cargo_target=${CARGO_TARGET_DIR:-target}
python3 tools/package-linux.py --binary "$cargo_target/release/hypercast" --media-prefix "${HYPERCAST_LINUX_MEDIA_DIR:?Build the media runtime first}"
