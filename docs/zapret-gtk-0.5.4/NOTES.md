# Zapret GTK 0.5.4: the design Sluis replaces

Screenshots taken 2026-10-24 on torOS (desktop PC), in a hidden headless labwc
session so the real desktop was not touched. Dark theme, window 450×515.
Nothing was applied: the zapret config and `~/.config/zapret-gtk` were
compared before and after and are unchanged.

## Captured screens

| File | Screen | What is on it |
|---|---|---|
| `01-found-strategies-page-1.png` | Main window, "Found Strategies" | Title, three-line explanation, one scrolling list of every strategy blockcheck found (111 on this PC) as raw nfqws arguments with a checkbox each; a row of buttons **1–10**; ⓘ, ⚙ and a green **Apply** |
| `02-found-strategies-list-end.png` | Same, scrolled to the end | Long two-line argument strings; the list is one page, not ten |
| `03-about-dialog.png` | ⓘ → About | Icon, "Zapret GTK 0.5.4", "Modern GTK4 interface for Zapret.", Website link, GPL-3.0 |
| `04-settings.png` | ⚙ → Settings | "Zapret Status: Running (Active)" + **Stop**; Import / Export; **Load Preset Strategies (Quick Setup)**; **opt** / **.config** / **json** (open folders/files); **Hostlist** / **Search Strategies** (amber) / **Delete Zapret** (red) |
| `05-hostlist.png` | Settings → Hostlist (Profile 1) | "If left empty, filtering will be applied to all websites"; one "Enter domain..." field and a **+**; **Save and Apply** |
| `06-search-confirm.png` | Settings → Search Strategies | Dialog: new strategies will overwrite Profile 1's saved ones; Cancel / Continue |
| `07-search-conflicts.png` | → Continue | "Conflicting Applications Detected! The following services must be stopped: nfqws"; **Stop and Continue** / **Continue Anyway** |
| `08-search-domains.png` | → Continue Anyway | "Enter the domain names of the websites you cannot access…"; domain field, **+**, **Start strategy search.** |

## Screens not captured, and why

They change the system or ask for root (a polkit prompt would have appeared on
the real screen), so they are described from the app's own strings instead:

- **Start strategy search** → "Select Scan Mode" ("Set a speed and scope level
  for the Blockcheck scan"), then a progress view ("Searching for
  strategies...", "Strategies Tried: N", cancel), then "New strategies found
  and saved to the selected profile…" or "Strategy Not Found".
- **Profiles 2–10**: switching logs "Applying profile N", so it likely rewrites
  `/opt/zapret/config`. An empty profile shows "No saved strategies found yet."
- **Apply**: "Strategies written to config file and Zapret service restarted."
  Warns on unsafe characters ("Security Warning: Strategy containing invalid or
  risky parameters detected").
- **Load Preset Strategies**: saves *and applies* at once ("Preset strategies
  have been saved and applied automatically"; "These strategies work in most
  cases, but may not work on every ISP or network").
- **Import / Export**: JSON file chooser ("Select Strategy File"; errors for a
  file that is not a JSON array).
- **Stop**: stops the service. **Delete Zapret**: "Are you sure you want to
  delete Zapret?" (Turkish text says found strategies are deleted too).
- **First run / install** (when `/opt/zapret` is missing): "Start to install and
  configure Zapret DPI bypass software", existing-installation and
  existing-folder questions, a Cloudflare DNS question ("WARNING: It does not
  work with the default service provider DNS"), DNSCrypt-proxy install, VPN
  conflict scan, progress messages, "Installation finished".
- **Update**: "A new Zapret update is available!" → download, rebuild, restart.

## Problems seen while using it (input for the redesign)

- **Raw arguments as the UI.** Rows are nfqws command lines
  (`--dpi-desync=fakeddisorder --dpi-desync-ttl=1 --dpi-desync-autottl=-3 …`).
  Nothing says which one works best, how they differ, or that ticking several
  does not combine them.
- **Ticking several strategies is broken.** Apply joins the ticked rows into one
  `NFQWS_OPT` without `--new`, so nfqws keeps only the last value of each
  option, a mix nobody tested. On this PC six ticked strategies became one that
  did not open Discord.
- **111 rows in one list**, about 220 Tab stops before the buttons; no search,
  no grouping, no "recommended".
- **"1–10" look like page numbers** but are profiles, unlabelled, and switching
  may apply one.
- **Actions that change the system with no confirmation**: Load Preset applies
  at once; Apply restarts the service.
- **Settings layout**: the bottom row (Hostlist / Search Strategies / Delete
  Zapret) is wider than the window and clipped at both edges (`04`); the
  destructive Delete sits next to the everyday Search; "opt", ".config" and
  "json" are developer paths, not user words.
- **Status is buried** in Settings; the main window does not say whether zapret
  is running or whether blocked sites currently open.
- **The strategy search** asks the user to type blocked domains but gives no
  examples from their region, and its "Conflicting Applications" page calls
  zapret's own nfqws a conflict.
- **Mixed languages**: the binary carries Turkish and English strings for the
  same screens (follows the system locale).
- No way to test a strategy against a site before applying it, and no
  per-site exclude list (the hostlist is "only these sites", not "all except").

## What torOS learned on 2026-10-24 (useful defaults)

On this ISP the working strategy for every site was
`--filter-tcp=80,443 --dpi-desync=fake --dpi-desync-ttl=8` (65/66 across 22
blocked and Turkish sites). autottl and `badseq,md5sig` broke nearby Turkish
sites. See `torOS/NEXT.md` (Appendix C, "zapret on the desktop PC") and
`torOS/build/zapret.conf`.
