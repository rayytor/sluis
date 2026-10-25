# Sluis

Open websites your internet provider blocks.

Sluis is a GNOME app for [Zapret](https://github.com/bol-van/zapret), the
tool by bol-van that gets around deep packet inspection. It is not a VPN:
nothing goes through anyone else's computer, and your connection stays as fast
as it is. Sluis replaces Zapret GTK 0.5.4 with a redesign for people who never
want to see an nfqws argument.

**Not technical? [Read the guide](docs/GUIDE.md)** — what Sluis does, how to
install it, and what to do when a site doesn't open, in plain words.

| | |
|---|---|
| ![Home: your sites at a glance](data/screenshots/home.png) | ![A scan found a way around the block](data/screenshots/scan-result.png) |
| Your sites at a glance | A way around the block, found in seconds |
| ![Methods, best first](data/screenshots/methods.png) | ![Choosing which sites Sluis works on](data/screenshots/sites.png) |
| Every method, best first | Choose which sites Sluis touches |

- **Your sites at a glance.** The home screen checks whether the sites you
  care about open, and says what to do when one doesn't.
- **Finds a way around a block in about a minute.** The quick scan tests
  methods in parallel without touching your running connection, and checks
  that sites which already worked still do. A Deep scan (Zapret's
  `blockcheck.sh`) is there when the quick one finds nothing.
- **Switches back by itself.** Pick another method and Sluis rechecks your
  sites; if one stops opening, it goes back to the previous method.
- **Remembers each network.** The method that works at home is used again
  when you get home.
- **Installs Zapret for you.** Get Started downloads the latest Zapret
  release, checks it against the published SHA-256 and sets it up. Updates
  keep your settings and roll back if Zapret won't start.

Status: **0.9.0, first preview.** It works day to day on the developer's
machine; it has not yet been tested on a clean install of every distribution.
English and Turkish.

## How it works

The window runs as you. Everything that needs root (writing
`/opt/zapret/config`, starting Zapret, running scans) goes through
`sluis-helper`, a small D-Bus service on the system bus that asks polkit
before each change. Methods are checked against the list of nfqws options
before they are written, config changes are atomic with a backup, and a
change that stops Zapret from starting is rolled back.

## Build and install

Needs Rust 1.85 or later, meson, and the GTK 4 (4.18+), libadwaita (1.8+) and
GLib development files (`gtk4-devel libadwaita-devel glib2-devel` on Fedora).

```sh
meson setup _build --prefix=/usr
meson compile -C _build
sudo meson install -C _build
```

The helper's D-Bus, polkit and systemd files always go where those services
look for them, so the helper also works from a `/usr/local` prefix.

Desktops without a polkit authentication agent can let members of `wheel`
change Zapret without a password: `meson setup _build -Dwheel_rule=true`.

To try the app without installing it (the helper must already be installed):

```sh
cargo run
```

On torOS, which keeps LLVM (and so `rustc`) off the host, build in the
container instead: `tools/container.sh cargo build`.

## Checks

`tools/check.sh` runs what CI runs: rustfmt, clippy, the tests, and the
desktop file, MetaInfo, GSettings schema and polkit/D-Bus validators.

## Translations

Translations are in `po/`, using gettext. Turkish is complete. To start a
new language, add it to `po/LINGUAS` and copy `po/sluis.pot` to
`po/<language>.po`.

## Layout

| Path | What |
|---|---|
| `src/` | The GTK app |
| `crates/core` | Strategy model: parses nfqws arguments, names them, reads and writes Zapret's config |
| `crates/scan` | Quick scan engine and the blockcheck output parser |
| `crates/helper` | `sluis-helper`, the privileged D-Bus service |
| `data/` | Desktop file, MetaInfo, GSettings schema, icons, UI, helper's system files |
| `po/` | Translations |
| `docs/` | [The guide for users](docs/GUIDE.md), [the design plan](docs/PLAN.md) and [notes on Zapret GTK 0.5.4](docs/zapret-gtk-0.5.4/NOTES.md) |
| `design/` | Mockups (still named Zapret), icon generators and the tools that render them |

## License

Sluis is free software under the [GNU General Public License, version 3 or
later](COPYING). Zapret itself is by bol-van, under its own license.
