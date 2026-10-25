#!/bin/sh
# One line per screen: ui file, output, logical width, height, options.
# Run from the design folder (shoot.sh does this).
r=tools/render
$r ui/01-home.ui              screens/01-home.png              520 820
$r ui/02-home-problem.ui      screens/02-home-problem.png      520 900
$r ui/03-strategies.ui        screens/03-strategies.png        520 820 --toast "Switched to Method 1 · All 4 sites open" --toast-button Undo
$r ui/04-strategy-details.ui  screens/04-strategy-details.png  520 820
$r ui/05-scan-sites.ui        screens/05-scan-sites.png        520 820
$r ui/06-scan-progress.ui     screens/06-scan-progress.png     520 820
$r ui/07-scan-result.ui       screens/07-scan-result.png       520 820
$r ui/08-sites.ui             screens/08-sites.png             520 820
$r ui/01-home.ui              screens/09-preferences.png       720 820 --dialog-ui ui/dialog-preferences.ui --page general
$r ui/01-home.ui              screens/09b-preferences-advanced.png 720 820 --dialog-ui ui/dialog-preferences.ui --page advanced
$r ui/10-welcome.ui           screens/10-welcome.png           520 820
$r ui/03-strategies.ui        screens/11-keep-strategy.png     520 820 --dialog-ui ui/dialog-keep.ui
$r ui/12-phone-new-network.ui screens/12-phone.png             360 720
$r ui/01-home.ui              screens/13-home-light.png        520 820 --light

# Contact sheet of everything above
python3 -I tools/sheet.py . > ui/sheet.ui
RENDER_SCALE=1.5 $r ui/sheet.ui zapret-design.png 1800 1750
