# Built by packaging/rpm.sh, which passes the version from meson.build.
%global debug_package %{nil}

Name:           sluis
Version:        %{pkgver}
Release:        1%{?dist}
Summary:        Open websites your internet provider blocks
License:        GPL-3.0-or-later
URL:            https://github.com/rayytor/sluis
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  cargo
BuildRequires:  rust
BuildRequires:  meson
BuildRequires:  gtk4-devel
BuildRequires:  libadwaita-devel
BuildRequires:  glib2-devel
BuildRequires:  gettext-devel
BuildRequires:  desktop-file-utils
BuildRequires:  systemd-rpm-macros

# the UI files use widgets newer than the symbols rpm picks up
Requires:       libadwaita >= 1.8
Requires:       gtk4 >= 4.18
Requires:       curl
Requires:       nftables
Requires:       tar
Requires:       polkit
Requires:       systemd

%description
Sluis is a GNOME app for Zapret, which gets around deep packet inspection.
It checks whether your sites open, finds a way around a block, and keeps it
working. It is not a VPN: your traffic does not go through anyone else's
server.

%prep
%autosetup

%build
%meson
%meson_build

%install
%meson_install
%find_lang %{name}

%post
%systemd_post sluis-helper.service
systemctl reload dbus.service >/dev/null 2>&1 || :

%preun
%systemd_preun sluis-helper.service

%postun
%systemd_postun_with_restart sluis-helper.service

%files -f %{name}.lang
%license COPYING
%doc README.md
%{_bindir}/sluis
%{_libexecdir}/sluis-helper
%{_datadir}/applications/io.github.rayytor.Sluis.desktop
%{_datadir}/metainfo/io.github.rayytor.Sluis.metainfo.xml
%{_datadir}/glib-2.0/schemas/io.github.rayytor.Sluis.gschema.xml
%{_datadir}/icons/hicolor/scalable/apps/io.github.rayytor.Sluis.svg
%{_datadir}/icons/hicolor/symbolic/apps/io.github.rayytor.Sluis-symbolic.svg
%{_datadir}/dbus-1/system.d/io.github.rayytor.Sluis.Helper.conf
%{_datadir}/dbus-1/system-services/io.github.rayytor.Sluis.Helper.service
%{_datadir}/polkit-1/actions/io.github.rayytor.Sluis.policy
%{_unitdir}/sluis-helper.service
