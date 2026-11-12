#!/bin/sh
set -eu
sudo apt-get update
sudo apt-get install -y build-essential pkg-config curl clang libssl-dev libglib2.0-dev \
    libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev gstreamer1.0-tools \
    gstreamer1.0-plugins-base gstreamer1.0-plugins-good gstreamer1.0-plugins-bad \
    gstreamer1.0-plugins-ugly gstreamer1.0-pipewire libavcodec-dev libavutil-dev \
    libswscale-dev libx11-dev libxrandr-dev libxfixes-dev libxdamage-dev libxi-dev \
    libxext-dev libxcomposite-dev libasound2-dev libpulse-dev libva-dev \
    libwayland-dev libxkbcommon-dev libegl1-mesa-dev libgl1-mesa-dev xvfb dbus-x11 gnupg "$@"
if ! command -v clang++-21 >/dev/null; then
    key="$(mktemp)"
    trap 'rm -f "$key"' EXIT
    curl --proto '=https' --tlsv1.2 --retry 3 --max-time 60 -fsSL \
        https://apt.llvm.org/llvm-snapshot.gpg.key -o "$key"
    gpg --show-keys --with-colons "$key" | grep -q '6084F3CF814B57C1CF12EFD515CF4D18AF4F7421'
    sudo install -m 644 "$key" /usr/share/keyrings/hypercast-llvm.asc
    echo 'deb [signed-by=/usr/share/keyrings/hypercast-llvm.asc] https://apt.llvm.org/noble/ llvm-toolchain-noble-21 main' |
        sudo tee /etc/apt/sources.list.d/hypercast-llvm.list >/dev/null
    sudo apt-get update
    sudo apt-get install -y clang-21
fi
