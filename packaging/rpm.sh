#!/bin/sh
# Builds dist/sluis.x86_64.rpm. Run on Fedora with the spec's BuildRequires
# and rpm-build installed (the release workflow does this in a container).
set -eu
cd "$(dirname "$0")/.."
version=$(sed -n "s/^  version: '\(.*\)',/\1/p" meson.build)
top=$PWD/target/rpmbuild
rm -rf "$top"
mkdir -p "$top/SOURCES" dist
tar -czf "$top/SOURCES/sluis-$version.tar.gz" \
	--exclude=./.git --exclude=./target --exclude='./_build*' \
	--exclude=./dist --exclude=./design \
	--transform "s|^\.|sluis-$version|" .
rpmbuild -bb --define "_topdir $top" --define "pkgver $version" packaging/sluis.spec
cp "$top"/RPMS/*/sluis-"$version"-*.rpm dist/sluis.x86_64.rpm
echo "dist/sluis.x86_64.rpm"
