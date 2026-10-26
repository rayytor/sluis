<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="design/logo/sluis-logo-dark.svg">
    <img src="design/logo/sluis-logo-light.svg" alt="Sluis" width="420">
  </picture>
</p>

Sluis is a GNOME app for [Zapret](https://github.com/bol-van/zapret), bol-van's
tool for getting past deep packet inspection. If your internet provider blocks
Discord or YouTube, Sluis finds a Zapret setting that gets through and keeps it
working. You never have to look at an nfqws argument.

It isn't a VPN. Your traffic doesn't go through anyone else's server, so your
connection is as fast as it was before.

If you just want to use it, go to **[rayytor.github.io/sluis](https://rayytor.github.io/sluis/)**
and download the package for your system. [The guide](docs/GUIDE.md) explains
the rest in plain language.

| | |
|---|---|
| ![Home: your sites at a glance](data/screenshots/home.png) | ![A scan found a way around the block](data/screenshots/scan-result.png) |
| Home screen | Scan result |
| ![Methods, best first](data/screenshots/methods.png) | ![Choosing which sites Sluis works on](data/screenshots/sites.png) |
| Methods | Site list |

The home screen checks the sites you care about and tells you which ones open.
When one doesn't, the quick scan tries methods in parallel without touching
your running connection, and usually finds one in about a minute. It also makes
sure the sites that already worked still do. If the quick scan comes up empty,
the deep scan runs Zapret's `blockcheck.sh`.

When you switch methods, Sluis rechecks your sites and goes back to the old
method if something stops opening. It remembers which method worked on which
Wi-Fi network. On first run it downloads the latest Zapret release, checks the
SHA-256 and installs it; updates keep your settings and roll back if Zapret
won't start.

This is 0.9.1, an early preview. I use it every day, but it hasn't been tried
on a clean install of every distro yet. English and Turkish.

## How it works

The window runs as your user. Anything that needs root (writing
`/opt/zapret/config`, starting Zapret, running scans) goes through
`sluis-helper`, a small D-Bus service on the system bus that asks polkit before
each change. Methods are validated against nfqws's option list before they're
written. Config writes are atomic and keep a backup, and if a change stops
Zapret from starting, it's rolled back.

## Building

You need Rust 1.92+, meson, and development files for GTK 4 (4.18+),
libadwaita (1.7+) and GLib. On Fedora that's
`gtk4-devel libadwaita-devel glib2-devel`.

```sh
meson setup _build --prefix=/usr
meson compile -C _build
sudo meson install -C _build
```

The helper's D-Bus, polkit and systemd files are always installed where those
services look for them, so a `/usr/local` prefix works too.

On a desktop with no polkit authentication agent, you can let members of
`wheel` change Zapret without a password: `meson setup _build -Dwheel_rule=true`.

To run the app without installing it (the helper still has to be installed):

```sh
cargo run
```

torOS keeps LLVM, and so `rustc`, off the host. Build in the container there:
`tools/container.sh cargo build`.

`tools/check.sh` runs the same checks as CI: rustfmt, clippy, the tests, and
the validators for the desktop file, MetaInfo, GSettings schema and
polkit/D-Bus files.

## Packages

`packaging/deb.sh` builds `dist/sluis_amd64.deb` on Debian 13, and the
package installs on Debian 13 or newer and Ubuntu 25.04 or newer.
`packaging/rpm.sh` builds `dist/sluis.x86_64.rpm` on Fedora 44 or newer.
Older releases ship a libadwaita older than 1.7.
Pushing a `v*` tag builds both in CI and publishes them as a GitHub release,
which is what the website's download links point at.

## Translations

Translations live in `po/` and use gettext. Turkish is complete. For a new
language, add it to `po/LINGUAS` and copy `po/sluis.pot` to `po/<lang>.po`.

## Layout

| Path | What |
|---|---|
| `src/` | The GTK app |
| `crates/core` | Strategy model: parses and names nfqws arguments, reads and writes Zapret's config |
| `crates/scan` | Quick scan engine and blockcheck output parser |
| `crates/helper` | `sluis-helper`, the privileged D-Bus service |
| `data/` | Desktop file, MetaInfo, GSettings schema, icons, UI, the helper's system files |
| `po/` | Translations |
| `docs/` | [User guide](docs/GUIDE.md) |
| `design/` | The logo and the scripts that draw the app icon |
| `packaging/` | The .deb and .rpm build scripts and the RPM spec |
| `website/` | The download page at rayytor.github.io/sluis |
| `tools/` | `check.sh` (the CI checks) and `container.sh` (build in a container) |

## License

GPL 3.0 or later, see [COPYING](COPYING). Zapret is bol-van's and has its own
license.
