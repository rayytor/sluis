#!/bin/sh
# Builds dist/sluis_amd64.deb. Run on Ubuntu 26.04 or newer (Sluis needs
# libadwaita 1.8) with the build dependencies installed; the release
# workflow does this in a container.
set -eu
cd "$(dirname "$0")/.."
version=$(sed -n "s/^  version: '\(.*\)',/\1/p" meson.build)
arch=$(dpkg --print-architecture)
work=$PWD/target/deb
root=$work/root
rm -rf "$work" _build-deb
mkdir -p "$root/DEBIAN" "$work/debian" dist

meson setup _build-deb --prefix=/usr --buildtype=release
meson compile -C _build-deb
DESTDIR="$root" meson install -C _build-deb --no-rebuild

install -Dm644 COPYING "$root/usr/share/doc/sluis/copyright"

# dpkg-shlibdeps wants a debian/control to sit next to
printf 'Source: sluis\n\nPackage: sluis\nArchitecture: any\n' >"$work/debian/control"
shlibs=$(cd "$work" && dpkg-shlibdeps -O "$root/usr/bin/sluis" "$root/usr/libexec/sluis-helper" |
	sed -n 's/^shlibs:Depends=//p' |
	# the symbols alone ask for less than the UI files use (AdwShortcutsDialog)
	sed -e 's/libadwaita-1-0 ([^)]*)/libadwaita-1-0 (>= 1.8)/' -e 's/libgtk-4-1 ([^)]*)/libgtk-4-1 (>= 4.18)/')

cat >"$root/DEBIAN/control" <<CONTROL
Package: sluis
Version: $version
Architecture: $arch
Maintainer: rayytor <rayytor@users.noreply.github.com>
Installed-Size: $(du -sk "$root/usr" | cut -f1)
Depends: $shlibs, curl, nftables, tar, polkitd, systemd, dbus
Section: net
Priority: optional
Homepage: https://rayytor.github.io/sluis/
Description: Open websites your internet provider blocks
 Sluis is a GNOME app for Zapret, which gets around deep packet inspection.
 It checks whether your sites open, finds a way around a block, and keeps it
 working. It is not a VPN: your traffic does not go through anyone else's
 server.
CONTROL

# the helper is started by D-Bus; systemd and the bus only need to reread
cat >"$root/DEBIAN/postinst" <<'SCRIPT'
#!/bin/sh
set -e
if [ -d /run/systemd/system ]; then
	systemctl daemon-reload || true
	systemctl reload dbus.service 2>/dev/null || true
fi
SCRIPT
cat >"$root/DEBIAN/prerm" <<'SCRIPT'
#!/bin/sh
set -e
if [ -d /run/systemd/system ]; then
	systemctl stop sluis-helper.service 2>/dev/null || true
fi
SCRIPT
cp "$root/DEBIAN/postinst" "$root/DEBIAN/postrm"
chmod 755 "$root/DEBIAN/postinst" "$root/DEBIAN/prerm" "$root/DEBIAN/postrm"

dpkg-deb --root-owner-group -Zxz --build "$root" dist/sluis_amd64.deb
echo "dist/sluis_amd64.deb"
