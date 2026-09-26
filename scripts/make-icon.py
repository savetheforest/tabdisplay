"""Draws the TabDisplay logo (1024x1024 PNG): a monitor with a tablet docked beside it, extending the screen.

Usage: python scripts/make-icon.py [out.png] [--full-bleed]
  default      macOS-style: rounded square inside a transparent margin (Apple's icon grid)
  --full-bleed the tile fills the canvas (Windows / general use)
Then: cd desktop && npx tauri icon ../icon.png  (generates every desktop size, .ico and .icns)
"""
import sys
from PIL import Image, ImageDraw, ImageFilter

S = 4096  # draw big, downscale for smooth edges
OUT = next((a for a in sys.argv[1:] if not a.startswith("--")), "icon.png")
FULL = "--full-bleed" in sys.argv

TOP, BOTTOM = (46, 110, 255), (124, 58, 237)  # blue -> violet
WHITE = (255, 255, 255, 255)


def lerp(a, b, t):
    return tuple(round(x + (y - x) * t) for x, y in zip(a, b))


def rounded_mask(size, box, radius):
    m = Image.new("L", size, 0)
    ImageDraw.Draw(m).rounded_rectangle(box, radius, fill=255)
    return m


img = Image.new("RGBA", (S, S), (0, 0, 0, 0))

# Tile: Apple grid puts an 824px squircle-ish tile in a 1024 canvas; full-bleed uses the whole canvas.
margin = 0 if FULL else round(S * 100 / 1024)
tile = (margin, margin, S - margin, S - margin)
tw = tile[2] - tile[0]
radius = round(tw * 0.225)

# Diagonal gradient, clipped to the rounded tile, with a soft shadow under it (macOS style only).
grad = Image.new("RGBA", (S, S))
gd = ImageDraw.Draw(grad)
for y in range(S):
    gd.line([(0, y), (S, y)], fill=lerp(TOP, BOTTOM, y / S) + (255,))
if not FULL:
    shadow = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    ImageDraw.Draw(shadow).rounded_rectangle((tile[0], tile[1] + S * 0.012, tile[2], tile[3] + S * 0.012), radius, fill=(0, 0, 0, 90))
    img = Image.alpha_composite(img, shadow.filter(ImageFilter.GaussianBlur(S * 0.012)))
img.paste(grad, (0, 0), rounded_mask((S, S), tile, radius))

# Subtle top highlight for depth.
hl = Image.new("RGBA", (S, S), (255, 255, 255, 0))
hd = ImageDraw.Draw(hl)
for y in range(tile[1], tile[1] + tw // 2):
    a = round(38 * (1 - (y - tile[1]) / (tw / 2)))
    hd.line([(tile[0], y), (tile[2], y)], fill=(255, 255, 255, a))
img = Image.alpha_composite(img, Image.composite(hl, Image.new("RGBA", (S, S), (0, 0, 0, 0)), rounded_mask((S, S), tile, radius)))

background = img.copy()  # the finished tile, used to cut the gap around the tablet
d = ImageDraw.Draw(img)
u = tw / 100  # layout unit: 1% of the tile
ox, oy = tile[0], tile[1]


def box(x0, y0, x1, y1, dy=3):  # dy: nudge the drawing down to its optical center
    return (ox + x0 * u, oy + (y0 + dy) * u, ox + x1 * u, oy + (y1 + dy) * u)


# Monitor (outline) with its stand.
stroke = round(5.2 * u)
d.rounded_rectangle(box(11, 18, 69, 58), round(5 * u), outline=WHITE, width=stroke)
d.rounded_rectangle(box(34, 58, 46, 68), round(1 * u), fill=WHITE)
d.rounded_rectangle(box(26, 66.5, 54, 71.5), round(2.5 * u), fill=WHITE)

# Tablet docked on the right, overlapping the monitor: solid white with a gradient "screen",
# and a dark gap around it so it reads as a separate device in front.
gap = 3.4
tab = (50, 45, 91, 77)  # landscape, like the tablet in use
back = Image.new("RGBA", (S, S), (0, 0, 0, 0))
ImageDraw.Draw(back).rounded_rectangle(box(tab[0] - gap, tab[1] - gap, tab[2] + gap, tab[3] + gap), round(7.5 * u), fill=(255, 255, 255, 255))
img = Image.composite(background, img, back.split()[3])  # the gap hides the monitor's lines under the tablet
d = ImageDraw.Draw(img)
d.rounded_rectangle(box(*tab), round(6 * u), fill=WHITE)
screen = box(tab[0] + 3.4, tab[1] + 3.4, tab[2] - 3.4, tab[3] - 3.4)
screen_grad = Image.new("RGBA", (S, S))
sd = ImageDraw.Draw(screen_grad)
for y in range(round(screen[1]), round(screen[3]) + 1):
    t = (y - screen[1]) / (screen[3] - screen[1])
    sd.line([(0, y), (S, y)], fill=lerp((96, 150, 255), (150, 100, 245), t) + (255,))
img.paste(screen_grad, (0, 0), rounded_mask((S, S), screen, round(3.2 * u)))

# The monitor's image "flows" onto the tablet: three bars continuing from the monitor's area.
d = ImageDraw.Draw(img)
for i, (y, length) in enumerate([(52.5, 22), (59.5, 15), (66.5, 19)]):
    d.rounded_rectangle(box(57.5, y, 57.5 + length, y + 3.6), round(1.8 * u), fill=(255, 255, 255, 235 - i * 25))

img.resize((1024, 1024), Image.LANCZOS).save(OUT)
print("saved", OUT)
