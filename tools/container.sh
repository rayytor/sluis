#!/bin/sh
# Runs a command in a Fedora 44 container that has Rust and the GTK and
# libadwaita development files (tools/Containerfile, built on first use).
# torOS keeps LLVM, and so rustc, off the host on purpose.
#   tools/container.sh cargo build
#   tools/container.sh tools/check.sh
set -eu
cd "$(dirname "$0")/.."
image=localhost/sluis-builder
# torOS has no systemd user manager to hand podman cgroups or a journal
podman() { command podman --cgroup-manager=cgroupfs --events-backend=file "$@"; }
podman image exists "$image" ||
	{ mkdir -p target/dnf-cache && podman build -v "$PWD/target/dnf-cache":/var/cache/libdnf5:Z -t "$image" -f tools/Containerfile tools; }
# same path inside as outside, so meson can install from the host what it
# built here (sudo meson install -C _build --no-rebuild)
podman run --rm -it \
	-v "$PWD":"$PWD":Z -w "$PWD" \
	-e CARGO_HOME="$PWD/target/cargo-home" \
	"$image" "$@"
