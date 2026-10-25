#!/bin/sh
# Everything CI checks, runnable locally: formatting, lints, tests, and the
# desktop file, MetaInfo and GSettings schema validators.
set -eu
cd "$(dirname "$0")/.."

cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
cp data/io.github.rayytor.Sluis.desktop.in "$tmp/io.github.rayytor.Sluis.desktop"
desktop-file-validate "$tmp/io.github.rayytor.Sluis.desktop"
cp data/io.github.rayytor.Sluis.metainfo.xml.in "$tmp/io.github.rayytor.Sluis.metainfo.xml"
appstreamcli validate --no-net --explain "$tmp/io.github.rayytor.Sluis.metainfo.xml"
glib-compile-schemas --strict --dry-run data
xmllint --noout data/helper/*.conf data/helper/*.policy
echo "all checks passed"
