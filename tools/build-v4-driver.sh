#!/usr/bin/env bash
set -euo pipefail
if [[ $# != 1 || "$1" != /* ]]; then
    echo 'Usage: tools/build-v4-driver.sh /absolute/install/prefix' >&2
    exit 2
fi
prefix=$1
revision=aed0ea19f3a273370a13c9009b96313c75d54c7b
for tool in git cmake pkg-config; do command -v "$tool" >/dev/null || { echo "Missing $tool; see docs/linux.md" >&2; exit 1; }; done
pkg-config --exists libusb-1.0 || { echo 'Missing libusb development files. Fedora: sudo dnf install libusb1-devel' >&2; exit 1; }
if [[ -e "$prefix" ]]; then echo "Refusing to overwrite existing prefix: $prefix" >&2; exit 1; fi
build_root=$(mktemp -d)
trap 'rm -rf -- "$build_root"' EXIT
git clone --no-checkout https://github.com/rtlsdrblog/rtl-sdr-blog.git "$build_root/source"
git -C "$build_root/source" checkout --detach "$revision"
cmake -S "$build_root/source" -B "$build_root/build" \
    -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$prefix" \
    -DCMAKE_INSTALL_LIBDIR=lib -DINSTALL_UDEV_RULES=OFF -DDETACH_KERNEL_DRIVER=ON
cmake --build "$build_root/build" --parallel 2
cmake --install "$build_root/build"
printf '%s\n' "$revision" > "$prefix/airwav-driver-source-commit.txt"
printf 'Built V4 driver at %s\n' "$prefix"
printf 'Run: airwav --library "%s/lib/librtlsdr.so" doctor\n' "$prefix"
