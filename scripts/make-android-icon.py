"""Writes the Android adaptive launcher icon (vector drawables) from the same geometry as make-icon.py.

Adaptive icons are 108x108dp with the visible safe zone in the middle 66dp, so the logo (drawn on a
100-unit tile in make-icon.py) is scaled into that zone. Parts of the monitor that sit under the tablet
are left out, which is what the "gap" around the tablet does in the PNG version.
Usage: python scripts/make-android-icon.py
"""
import os

RES = os.path.join(os.path.dirname(__file__), "..", "android", "app", "src", "main", "res")
K = 0.6          # logo units -> dp (fits the round mask with a margin)
OX, OY = 23.4, 23.7  # centers the logo (units 11..91 x 21..80) in the 108dp canvas
DY = 3           # same optical nudge as make-icon.py


def X(x):
    return round(OX + x * K, 2)


def Y(y):
    return round(OY + (y + DY) * K, 2)


def rrect(x0, y0, x1, y1, r):
    """Closed rounded-rectangle path in logo units."""
    x0, y0, x1, y1, r = X(x0), Y(y0), X(x1), Y(y1), round(r * K, 2)
    return (f"M{x0 + r},{y0} H{x1 - r} A{r},{r} 0 0 1 {x1},{y0 + r} V{y1 - r} A{r},{r} 0 0 1 {x1 - r},{y1} "
            f"H{x0 + r} A{r},{r} 0 0 1 {x0},{y1 - r} V{y0 + r} A{r},{r} 0 0 1 {x0 + r},{y0} Z")


# Geometry (logo units), mirrors make-icon.py.
MON = (11, 18, 69, 58)
STROKE, MON_R = 5.2, 5
TAB = (50, 45, 91, 77)
GAP = 3.4
cut_x, cut_y = TAB[0] - GAP, TAB[1] - GAP  # monitor lines stop here, before the tablet

# The monitor outline as an open stroke: top, left and the visible parts of the bottom and right edges.
h = STROKE / 2
mx0, my0, mx1, my1, r = MON[0] + h, MON[1] + h, MON[2] - h, MON[3] - h, MON_R - h
monitor = (f"M{X(mx0)},{Y(my1 - r)} V{Y(my0 + r)} A{r * K},{r * K} 0 0 1 {X(mx0 + r)},{Y(my0)} "
           f"H{X(mx1 - r)} A{r * K},{r * K} 0 0 1 {X(mx1)},{Y(my0 + r)} V{Y(cut_y)} "
           f"M{X(mx0)},{Y(my1 - r)} A{r * K},{r * K} 0 0 0 {X(mx0 + r)},{Y(my1)} H{X(cut_x)}")
stand = rrect(34, 58, 46, 68, 1)
base = rrect(26, 66.5, cut_x, 71.5, 2.5)
tablet = rrect(*TAB, 6)
screen = rrect(TAB[0] + 3.4, TAB[1] + 3.4, TAB[2] - 3.4, TAB[3] - 3.4, 3.2)
bars = [rrect(57.5, y, 57.5 + length, y + 3.6, 1.8) for y, length in [(52.5, 22), (59.5, 15), (66.5, 19)]]

HEAD = '<?xml version="1.0" encoding="utf-8"?>\n'


def vector(body, extra_ns=""):
    return (HEAD + f'<vector xmlns:android="http://schemas.android.com/apk/res/android"{extra_ns}\n'
            '    android:width="108dp" android:height="108dp"\n'
            '    android:viewportWidth="108" android:viewportHeight="108">\n' + body + "</vector>\n")


def path(d, fill=None, stroke=None, width=None, alpha=None):
    attrs = [f'android:pathData="{d}"']
    if fill:
        attrs.append(f'android:fillColor="{fill}"')
    if stroke:
        attrs += [f'android:strokeColor="{stroke}"', f'android:strokeWidth="{round(width * K, 2)}"', 'android:strokeLineCap="butt"']
    if alpha is not None:
        attrs.append(f'android:fillAlpha="{alpha}"')
    return "    <path " + "\n        ".join(attrs) + " />\n"


screen_gradient = (
    f'    <path android:pathData="{screen}">\n'
    '        <aapt:attr name="android:fillColor">\n'
    f'            <gradient android:type="linear" android:startX="0" android:startY="{Y(TAB[1])}" android:endX="0" android:endY="{Y(TAB[3])}"\n'
    '                android:startColor="#6096FF" android:endColor="#9664F5" />\n'
    "        </aapt:attr>\n"
    "    </path>\n"
)
foreground = vector(
    path(monitor, stroke="#FFFFFF", width=STROKE)
    + path(stand, fill="#FFFFFF")
    + path(base, fill="#FFFFFF")
    + path(tablet, fill="#FFFFFF")
    + screen_gradient
    + "".join(path(b, fill="#FFFFFF", alpha=a) for b, a in zip(bars, (0.92, 0.82, 0.72))),
    extra_ns='\n    xmlns:aapt="http://schemas.android.com/aapt"',
)
# Themed icons (Android 13+) tint this single-color version; the screen becomes a hole.
monochrome = vector(
    path(monitor, stroke="#FFFFFF", width=STROKE)
    + path(stand, fill="#FFFFFF")
    + path(base, fill="#FFFFFF")
    + path(tablet + " " + screen, fill="#FFFFFF").replace("/>", 'android:fillType="evenOdd" />')
    + "".join(path(b, fill="#FFFFFF") for b in bars)
)
background = (HEAD + '<shape xmlns:android="http://schemas.android.com/apk/res/android">\n'
              '    <gradient android:angle="270" android:startColor="#2E6EFF" android:endColor="#7C3AED" />\n'
              "</shape>\n")
adaptive = (HEAD + '<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">\n'
            '    <background android:drawable="@drawable/ic_launcher_background" />\n'
            '    <foreground android:drawable="@drawable/ic_launcher_foreground" />\n'
            '    <monochrome android:drawable="@drawable/ic_launcher_monochrome" />\n'
            "</adaptive-icon>\n")

files = {
    "drawable/ic_launcher_foreground.xml": foreground,
    "drawable/ic_launcher_monochrome.xml": monochrome,
    "drawable/ic_launcher_background.xml": background,
    "mipmap-anydpi/ic_launcher.xml": adaptive,
    "mipmap-anydpi/ic_launcher_round.xml": adaptive,
}
for name, content in files.items():
    full = os.path.join(RES, name)
    os.makedirs(os.path.dirname(full), exist_ok=True)
    open(full, "w", encoding="utf-8", newline="\n").write(content)
    print("wrote", os.path.relpath(full))
