# Sluis 1.0: design plan

Goal: a GNOME HIG-compliant GTK4/libadwaita app that a non-technical user can
open, press one button, and get blocked sites working in about a minute,
without ever seeing an nfqws argument unless they ask for it.

Basis: the installed Zapret in `/opt/zapret` (`config`, `blockcheck.sh`) on
this PC and the GNOME HIG.

---

## 2. Target UX

### 2.1 Information architecture

```
Main window (AdwApplicationWindow → AdwToolbarView → AdwNavigationView)
├── Home (status)                         ← default page
│   ├── Protection        [switch]        "Active · Fake packet, TTL 8"
│   ├── Your sites        live ✓/✗ per site, "Check Now"
│   └── Strategy          → Strategies page
│       "Find a Better Strategy…"  (pill, suggested-action)
├── Strategies page       ranked list, search, single choice
├── Sites page            filtering mode + list (was "Hostlist")
└── Scan flow             Sites → Scanning → Result (pushed pages)

Primary menu: Import Strategies… / Export Strategies… / Preferences /
              Keyboard Shortcuts / About Sluis
Preferences (AdwPreferencesDialog): General · Network · Advanced
```

### 2.2 Home page

- `AdwHeaderBar` with `AdwWindowTitle` ("Zapret", subtitle = current network
  name) and the primary menu button on the right. Nothing else in the header.
- `AdwBanner` for exceptional states only: "Zapret is not installed" (button:
  Install), "Update available" (Update), "Network changed. No strategy saved
  for this network" (Scan), "Service failed to start" (Details).
- Content is an `AdwPreferencesPage` (clamped, adaptive for free):
  - **Protection** `AdwSwitchRow`. Subtitle: "Active · Fake packet, TTL 8" or
    "Off". Toggling never needs a confirmation (it is instantly reversible).
  - **Your Sites** group: one `AdwActionRow` per site the user cares about,
    suffix = status icon (`emblem-ok-symbolic` / `dialog-warning-symbolic`) +
    latency; header suffix button "Check Now". This is the "is it working?"
    answer the old app never gave.
  - **Strategy** `AdwActionRow` with `go-next-symbolic`, opens the Strategies
    page.
  - Pill button "Find a Better Strategy…" (`suggested-action` only when a site
    is failing; plain otherwise).
- Empty/first run: `AdwStatusPage` "Set Up Zapret", one sentence, button
  "Set Up…". Installation progress shown in place, then flow straight into the
  quick scan.

### 2.3 Strategies page

- Generated, human names from a parsed strategy model, not raw args:
  `--dpi-desync=fake --dpi-desync-ttl=8` → **Fake packet, TTL 8**;
  `fakeddisorder … split-pos=1` → **Fake + reordered split at byte 1**.
- `GtkListView` + `AdwActionRow`-style rows with a radio (`GtkCheckButton`
  group): **single selection**. Subtitle: "22 of 22 sites · 48 ms · tested
  today". "Recommended" tag on the top-ranked row.
- Expand (`AdwExpanderRow` or row → detail page) shows raw arguments in a
  monospace, selectable label with a Copy button, plus per-site results.
- `GtkSearchBar` (Ctrl+F) over names and args. Sections: "Recommended",
  "Works for all your sites", "Partial".
- Choosing a row applies it (no separate Apply button) with the **auto-revert
  pattern** borrowed from GNOME Display settings: apply → re-check sites →
  if all pass, toast "Strategy applied" with Undo; if checks fail, an
  `AdwAlertDialog` "Keep this strategy?" with a 15 s countdown, default
  "Revert".
- Advanced users: "Custom Strategy…" row opens a dialog with an
  `AdwEntryRow` validated live by the parser (error state + message, never a
  regex "security warning").
- "Different strategies per site" (the real use case behind multi-select) is
  an explicit advanced feature: per-site rules compiled with `--new` and
  `--hostlist-domains`, never an accidental merge.

### 2.4 Sites page (replaces "Hostlist (Profile 1)")

- `AdwComboRow` **Apply To**: "All sites" / "Only these sites" /
  "All sites except these" (adds the missing exclude list, `--hostlist-exclude`).
- List of `AdwActionRow`s with remove buttons; `AdwEntryRow` "Add Site" that
  accepts pasted URLs and normalises them (`https://www.discord.com/x` →
  `discord.com`), with inline validation.
- Same list feeds "Your Sites" checks and the scan, so the user types domains
  once.
- Saving is implicit (HIG: preferences apply instantly); the service reload is
  debounced and shown as a toast.

### 2.5 Profiles → Networks

Replace the ten numbered profiles with **automatic per-network strategies**:
keyed by NetworkManager connection UUID (fallback: gateway MAC). On network
change, the saved strategy for that network is applied; unknown network shows
the banner offering a quick scan. Manual named profiles are not needed for v1;
if kept, they live in Preferences → Network as an `AdwComboRow`.

### 2.6 Preferences (`AdwPreferencesDialog`, searchable)

- **General**: Start at login/boot (`AdwSwitchRow`), Switch strategy when the
  network changes, Notify when a site stops working.
- **Network**: Ports (TCP/UDP `AdwEntryRow`s with validation), Bypass QUIC /
  HTTP/3 (Discord voice, YouTube), IPv6, Encrypted DNS (explain *why* in a
  subtitle instead of a "WARNING").
- **Advanced**: Show raw arguments in lists, Open Configuration Folder (one
  row, not "opt/.config/json"), Reset to Defaults…, **Uninstall Zapret…**
  at the very bottom in its own group, `destructive-action`, with an
  `AdwAlertDialog` listing exactly what is removed.

### 2.7 Dialogs, feedback and copy rules

- Confirm only what is destructive or hard to undo (overwrite saved results,
  uninstall). Everything else: do it + `AdwToast` with Undo.
- `AdwAlertDialog` only, heading in header case, body one or two sentences,
  responses as verbs ("Replace", "Uninstall"), Cancel default and never
  colored.
- No custom colors anywhere: only `suggested-action`, `destructive-action`,
  `pill`, `flat`, `success`/`warning`/`error` style classes, so light/dark
  and accent colors work.
- Errors say what happened and what to do: "Could not restart Zapret. The
  previous strategy is still active." with a Details expander for the log.
- About: `AdwAboutDialog` filled from the MetaInfo (release notes, credits to
  bol-van/zapret, issue tracker, debug info = versions + config).

### 2.8 Adaptive layout and accessibility

- Works from 360×294 (GNOME phone minimum) upward; `AdwBreakpoint` for any
  wide layout; nothing has a fixed width (fixes the clipped Settings row).
- Lists are `GtkListView`/boxed lists: one Tab stop into a list, arrows within.
- Every icon-only button has a tooltip and accessible label; status icons have
  accessible descriptions ("discord.com: opens, 48 ms").
- Shortcuts: Ctrl+F search, Ctrl+, Preferences, Ctrl+R check sites,
  Ctrl+Shift+S scan, Ctrl+? shortcuts, Ctrl+Q / Ctrl+W.
- Test with Orca, high contrast, large text, keyboard only.

### 2.9 Internationalisation

gettext with `.po` files (English source, Turkish first translation); no
strings compiled per language; plural forms; numbers/times via GLib.

---

## 3. Quick scanning (the core feature)

Target: **under 60 s** on a typical connection to a working, verified strategy,
with the full blockcheck kept as an optional "Thorough Scan".

### 3.1 Why blockcheck is slow

`blockcheck.sh` tests each domain × protocol (HTTP, TLS 1.2, TLS 1.3, QUIC)
× a long strategy matrix (including TTL 1–12 and autottl sweeps), **one at a
time**, restarting nfqws for every attempt, `CURL_MAX_TIME=2` each. That is
hundreds of serial attempts.

### 3.2 Quick scan pipeline

1. **Diagnose (≈3 s).** For each target site, in parallel, without any bypass:
   resolve via system DNS and via DoH and compare (DNS poisoning?), then
   HTTP, TLS 1.2, TLS 1.3 and optionally QUIC. Classify per site: *open*,
   *DNS spoofed*, *TLS reset/timeout (DPI)*, *block page*, *unreachable*.
   - All open → "These sites already open. Zapret isn't needed for them."
   - Only DNS spoofing → offer encrypted DNS, no DPI strategy needed.
   - Picks which protocol(s) the candidates must fix.
2. **Candidates in priority order:**
   1. last known-good strategy for this network (instant re-validation);
   2. curated shortlist (~20) from real-world success (e.g. `fake ttl=8`, `fake fooling=ts`, `multisplit`, `fakedsplit`,
      `fakeddisorder split-pos=1`, `hostfakesplit`, `syndata`), ordered by a
      shipped JSON that can be updated without a new release;
   3. TTL sweep for fake-based families (TTL 1–12 tested **in parallel**, not
      sequentially).
3. **Parallel isolated testing.** Run N (default 8) private `nfqws` instances,
   each on its own queue (`--qnum 200+i`), and steer test traffic to them by
   **source port**: test requests are made with `curl --local-port
   (40000+100·i)-(40000+100·i+99)`, and a temporary nftables table
   (`sluis_scan`) queues `tcp sport` / `udp sport` ranges to the
   matching queue. Normal traffic and the running zapret service are not
   touched; zapret's own nfqws is **not** a conflict. Fallback when the rule
   cannot be isolated: pause the service for the scan; the helper guarantees
   restoration even if the UI crashes.
4. **Pass criteria per candidate:** every *failing* target site succeeds on
   the protocols that failed in step 1, **twice** (flakiness guard), and a
   small set of **control sites that were already open still open** (this
   catches the autottl/`badseq,md5sig` regressions seen on Turkish sites).
   Block-page detection: redirect to a known ISP host or response body
   fingerprint, not just HTTP 200.
5. **Early exit.** Stop at the first candidate that passes; show the result
   page immediately with "Apply" and "Keep Looking" (continues in the
   background to rank more options).
6. **Ranking** for the Strategies page: sites passed → control sites intact →
   median latency → simplicity (fewer options = more robust).
7. **Escalation.** Nothing in the shortlist passes → offer "Thorough Scan"
   (wraps `blockcheck.sh` with `SCANLEVEL`, `SKIP_*` and the user's domains,
   streams its output, parses `!!!!! AVAILABLE !!!!!` lines into the same
   model). Runs in the helper so the window can close; `GNotification` when
   done.

### 3.3 Scan UI flow (pushed `AdwNavigationPage`s)

1. **Which sites don't open?** Prefilled from the Sites list plus suggestion
   chips based on locale/region (Discord, YouTube, X, Roblox…), paste-friendly
   entry. Primary button "Scan".
   (No overwrite confirmation: results are *added* and ranked; nothing is lost.)
2. **Scanning.** `GtkProgressBar` with real fraction, current step in plain
   words ("Testing fake packets with different TTLs…"), a live list of
   per-site results, "Stop" in the header bar. Survives navigation away.
3. **Result.** `AdwStatusPage` "Found a Working Strategy", name, "4 of 4 sites
   open · 52 ms", "Apply" (suggested) + "Keep Looking". On failure: "No Quick
   Fix Found" with "Run Thorough Scan" and an honest time estimate.

---

## 4. Architecture

```
sluis (workspace, Rust)
├── crates/core     nfqws arg parser + typed Strategy model, human names,
│                   lossless /opt/zapret/config reader/writer, validation,
│                   site normalisation, per-network store (serde JSON)
├── crates/scan     diagnose, candidate list, parallel runner, scoring,
│                   blockcheck output parser; no GTK dependency
├── crates/helper   privileged system D-Bus service (zbus), polkit-checked
│                   methods: Apply, SetEnabled, StartScan/StopScan,
│                   Install/Update/Uninstall; owns nfqws test processes and
│                   nft scan table, cleans up on exit/crash (watchdog)
└── app             GTK4 + libadwaita UI, Blueprint .blp files, GResource,
                    GSettings schema, gettext
```

Key decisions:

- **Unprivileged UI + polkit helper** instead of running the GUI as root or
  calling `pkexec` per action. Fine-grained polkit actions
  (`…apply`, `…scan`, `…manage`) with `auth_admin_keep`, so one password
  covers a session.
- Arguments always passed as `Vec<String>` to `Command`, never through a shell;
  the parser whitelists known nfqws options and value types. This replaces the
  "unsafe characters" heuristic.
- **Config writes are transactional**: parse, change only managed keys
  (`NFQWS_OPT`, `NFQWS_PORTS_*`, `MODE_FILTER`), write to temp, fsync, rename,
  keep `config.sluis.bak`; restart service; health-check; roll back on
  failure.
- UI code: plain gtk-rs subclassing + Blueprint (the GNOME Circle norm), async
  via `glib::spawn_future_local`, helper signals for progress. Keep
  `json-glib` out; use serde.
- App ID `io.github.rayytor.Sluis`; MetaInfo with screenshots and
  releases; desktop file; full-color app icon + symbolic icon per GNOME icon
  guidelines (the current one is a generic download glyph).
- Packaging: RPM for torOS/Fedora first (helper, polkit policy, D-Bus service
  file, systemd unit). Flatpak only for the UI with the helper on the host
  is possible later; nftables/nfqws cannot live inside the sandbox.

---

## 5. Milestones

| # | Milestone | Done when |
|---|---|---|
| M0 | Skeleton | Workspace, GtkBuilder `.ui` (not Blueprint: the mockups are already `.ui` and render in real GTK), GResource, GSettings, gettext, app ID `io.github.rayytor.Sluis`, MetaInfo, final app icon, CI (fmt, clippy, tests, `appstreamcli validate`, `desktop-file-validate`); torOS builds in `tools/container.sh` |
| M1 | Core model | Parser round-trips every strategy blockcheck produced on this PC (the 111) and names them; config writer is lossless (diff test against `/opt/zapret/config`). **Done 2026-10-24.** Found: the 111 are 63 distinct strategies (blockcheck reports one per passed test; the app must merge them, keeping which tests each passed), and the live config's `TPWS_SOCKS_OPT`/`TPWS_OPT` are damaged (`Config::problems` reports it; repair belongs to the helper, M2) |
| M2 | Helper | D-Bus + polkit; Apply with rollback; SetEnabled; status signal; crash cleanup tested. **Done 2026-10-24**, verified live on the desktop PC: Repair fixed the damaged config, a strategy nfqws could not start was rolled back, on/off, idle exit. torOS has no polkit agent, so it installs with `-Dwheel_rule=true`. Scan cleanup (nft table, test nfqws) arrives with M4 |
| M3 | Home + Strategies + Sites | Pages from §2 working against the helper; auto-revert on apply; toasts with Undo. **Done 2026-10-24**, verified live: real checks, automatic switch-back after 12 s, Undo, import of the 63 old strategies |
| M4 | Quick scan engine | Diagnose + parallel source-port isolation + scoring; on this PC finds `fake ttl=8` for the 22-site list in < 60 s with control sites intact. **Done 2026-10-24**, verified live through the helper: 18.7 s and 20.2 s, `fake ttl=8` opened Discord, Roblox, Wattpad and Pastebin twice with all 18 other sites intact, zapret's own nfqws untouched. Found: curl 8.18 counts the TLS handshake as connecting, so a stalled handshake needs a separate TCP check; this ISP's DPI slows down with many parallel handshakes (0.8 s at 4, 2.4 s at 32, even through zapret), so requests are capped at 12; `fooling=md5sig` opens the blocked sites but breaks nine Turkish ones, which the control sites catch; only TCP/HTTPS over IPv4 is tested so far |
| M5 | Scan UI + Thorough Scan | Flow from §3.3; blockcheck wrapper and parser; notifications. **Done 2026-10-24**, verified live (dialog driven through its window actions in a headless session): quick scan from the dialog found the method in use in 18–21 s; the Thorough Scan pauses zapret (restored on Stop, on finish and after a helper crash), and blockcheck found `fake ttl=8` for Discord after 88 attempts (4 min 51 s). A full Thorough Scan to its end was not run |
| M6 | Onboarding, update, uninstall, per-network switching | First run to working state without touching a terminal. **Built 2026-10-24**: Get Started installs the latest upstream release (SHA-256 from GitHub checked, unpacked beside the old copy, then swapped in) and opens the scan; Update keeps config and lists and rolls back if zapret won't start; Uninstall only removes what the app installed; distribution packages (torOS) are left to the system. Per network: keyed by the router's MAC, named via NetworkManager or iwd, method remembered once every site opens, applied with Undo on return. Verified live: kind "system", latest 72.13, this Wi-Fi remembered by name; leaving and rejoining the Wi-Fi switched to the method saved for it and Undo switched back; Preferences shows "Comes with your system" with Uninstall hidden. Install, update and uninstall ran for real in a Fedora 44 container (`examples/install_live.rs`) against GitHub's 72.13: downloaded and verified in 22.6 s, upstream's install_bin.sh picked the x86-64 nfqws, update kept config and lists, uninstall left nothing; only starting the systemd unit was not covered (no systemd there; the torOS VM image of 09:01 is the desktop build and does not boot in QEMU, and this kernel has no KVM) |
| M7 | Polish & audit | HIG checklist below all green; Orca pass; 360 px pass; Turkish translation; screenshots for MetaInfo. **Done 2026-10-24** except the live Orca pass (see §6): methods search, Ctrl+Shift+S, screen-reader announcements and labels, 360×294 checked on every page, Turkish 253/253 checked in the running app, Türkiye detected from the time zone, real MetaInfo screenshots; scans now prefer the method in use when it passes |

M1 and M2 are independent and can be built in parallel; M3 needs both; M4
needs M2; M5 needs M3 + M4.

---

## 6. HIG compliance checklist (release gate)

Audited 2026-10-24 (M7). [x] = checked, with how.

- [x] Only libadwaita widgets/patterns for structure: `AdwApplicationWindow`,
      `AdwToolbarView`, `AdwHeaderBar`, `AdwNavigationView`,
      `AdwPreferencesDialog`, `AdwAlertDialog`, `AdwAboutDialog`,
      `AdwBanner`, `AdwToast`, boxed lists (the heroes are clamped boxes
      like `AdwStatusPage`, so they can sit above the lists)
- [x] Primary menu contains Preferences, Keyboard Shortcuts, About; no
      About/Settings buttons in content
- [x] No hard-coded colors; the only CSS (hero circle, "In Use" tag) uses
      libadwaita's color variables
- [x] Header capitalisation for titles/buttons, sentence case for
      descriptions; ellipsis on "Uninstall Zapret…"; no "!"
- [x] At most one `suggested-action` per view; Uninstall in its own group,
      `destructive-action`, confirmed with what it removes
- [x] Instant-apply preferences; no Save buttons
- [x] Adaptive to 360×294: every page and dialog rendered at that size,
      nothing clipped, dialogs become bottom sheets
- [x] Full keyboard operation; shortcuts window (Ctrl+R check, Ctrl+Shift+S
      Unblock Sites, Ctrl+F search methods, Ctrl+, Ctrl+? Ctrl+W Ctrl+Q);
      focus order checked by tabbing through Preferences
- [~] Accessible names on all icon-only buttons (tooltips; remove buttons
      name their site), status icons decorative next to their text, home
      status and scan start/result announced. **Not done:** a live Orca
      pass, which needs a real session with AT-SPI and audio
- [x] Empty, loading and error states: home (loading, helper missing, off,
      no sites, checking), methods search ("No methods match"), scan
      (helper unreachable, scan failed, nothing blocked, DNS, unreachable)
- [x] All user-facing strings translatable; Turkish complete (253/253)
- [x] MetaInfo, desktop file, icons validate (tools/check.sh); About shows
      release notes; screenshots are the real app (data/screenshots)

---

## 7. Open questions

1. Keep manual profiles at all, or only per-network automatic ones?
2. Ship a curated strategy shortlist per country (needs a small data source
   and update channel), or one global list ranked locally?
3. Should the helper also manage encrypted DNS (systemd-resolved DoT config),
   or only recommend it?
4. ~~Name/branding?~~ Decided
   2026-10-24: **Sluis** (Dutch for a sluice or lock: it lets traffic
   through). App ID `io.github.rayytor.Sluis`; "Zapret" now only names the
   engine underneath.
