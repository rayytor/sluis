#!/bin/sh
# Renders every mockup in a hidden headless labwc session, so the real desktop
# is never touched. Output: ../screens/*.png
set -eu
here=$(cd "$(dirname "$0")" && pwd)
design=$(dirname "$here")
list=$(realpath "${1:-$here/screens.sh}")  # which screens to render
cfg=$(mktemp -d)               # empty labwc + GTK config: no user autostart, stock look
mkdir -p "$cfg/xdg"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
export WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=pixman

cat > "$cfg/inner.sh" <<EOF
#!/bin/sh
export MOCKUP_CSS="$design/ui/style.css" MOCKUP_ICONS="$design/icons" GTK_A11Y=none GDK_BACKEND=wayland GSETTINGS_BACKEND=memory MOCKUP_STOCK=1 GDK_DEBUG=no-portals XDG_CONFIG_HOME="$cfg/xdg"
wlr-randr --output HEADLESS-1 --custom-mode 3200x2400 --scale 2 2>/dev/null || true
cd "$design"
dbus-run-session -- sh "$list" > "$cfg/log" 2>&1
echo done >> "$cfg/log"
labwc --exit
EOF
chmod +x "$cfg/inner.sh"
unset WAYLAND_DISPLAY DISPLAY
labwc -C "$cfg" -s "$cfg/inner.sh" >/dev/null 2>&1 || true
cat "$cfg/log"
rm -rf "$cfg"
