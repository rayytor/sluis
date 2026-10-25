Test data from the desktop PC, 2026-10-24.

- `strategies-blockcheck.txt`: the 111 strategies blockcheck found there, as
  the old app stored them (`~/.config/zapret-gtk/strategies.json`), one
  per line.
- `config.default`: zapret 72.8's stock `/opt/zapret/config.default`.
- `config.toros-damaged`: the live `/opt/zapret/config`. Its `TPWS_SOCKS_OPT`
  and `TPWS_OPT` lost their multi-line values and closing quotes, so each
  quoted value now swallows the lines up to the next quote. Kept as found:
  the reader must round-trip it and report the damage.
