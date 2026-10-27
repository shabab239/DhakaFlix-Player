"""Draws the DhakaFlix app icon: a golden 'play' sun held by a shapla (water lily) on a dusk sky.

uv run --with pillow python make_icon.py <out_dir>
Writes icon.ico (16..256) and icon-256.png.
"""
import math, sys, os
from PIL import Image, ImageDraw, ImageFilter, ImageChops

S = 1024
out = sys.argv[1] if len(sys.argv) > 1 else "."


def lerp(a, b, t):
    return tuple(round(a[i] + (b[i] - a[i]) * t) for i in range(len(a)))


def vgradient(w, h, stops):
    """stops: [(pos 0..1, (r,g,b,a)), ...]"""
    col = Image.new("RGBA", (1, h))
    for y in range(h):
        t = y / (h - 1)
        for (p0, c0), (p1, c1) in zip(stops, stops[1:]):
            if p0 <= t <= p1:
                col.putpixel((0, y), lerp(c0, c1, (t - p0) / max(p1 - p0, 1e-6)))
                break
    return col.resize((w, h))


def radial(size, inner, outer, power=1.0):
    img = Image.new("RGBA", (size, size))
    px = img.load()
    c = (size - 1) / 2
    for y in range(size):
        for x in range(size):
            d = min(1.0, math.hypot(x - c, y - c) / c)
            px[x, y] = lerp(inner, outer, d ** power)
    return img


canvas = Image.new("RGBA", (S, S), (0, 0, 0, 0))

# --- tile: dusk sky -------------------------------------------------------------------------
M = 28
tile_mask = Image.new("L", (S, S), 0)
ImageDraw.Draw(tile_mask).rounded_rectangle((M, M, S - M, S - M), radius=228, fill=255)
sky = vgradient(S, S, [
    (0.00, (34, 22, 78, 255)),
    (0.45, (120, 38, 120, 255)),
    (0.78, (232, 84, 104, 255)),
    (1.00, (255, 140, 96, 255)),
])
canvas.paste(sky, (0, 0), tile_mask)

# a few stars in the upper sky
stars = Image.new("RGBA", (S, S), (0, 0, 0, 0))
sd = ImageDraw.Draw(stars)
for x, y, r, a in [(190, 180, 7, 200), (300, 120, 4, 150), (820, 170, 6, 190), (880, 300, 4, 140),
                   (150, 330, 4, 120), (720, 110, 3, 130), (610, 200, 3, 110)]:
    sd.ellipse((x - r, y - r, x + r, y + r), fill=(255, 240, 220, a))
canvas = Image.alpha_composite(canvas, stars.filter(ImageFilter.GaussianBlur(1.2)))

# --- sun glow + golden disc ----------------------------------------------------------------
cx, cy, R = 512, 375, 215
glow = Image.new("RGBA", (S, S), (0, 0, 0, 0))
ImageDraw.Draw(glow).ellipse((cx - R - 70, cy - R - 70, cx + R + 70, cy + R + 70), fill=(255, 196, 120, 120))
canvas = Image.alpha_composite(canvas, glow.filter(ImageFilter.GaussianBlur(48)))

disc = radial(2 * R, (255, 236, 170, 255), (255, 150, 84, 255), power=1.6)
dmask = Image.new("L", (2 * R, 2 * R), 0)
ImageDraw.Draw(dmask).ellipse((0, 0, 2 * R - 1, 2 * R - 1), fill=255)
layer = Image.new("RGBA", (S, S), (0, 0, 0, 0))
layer.paste(disc, (cx - R, cy - R), dmask)
canvas = Image.alpha_composite(canvas, layer)

# play triangle, rounded corners (polygon + thick curved outline in the same colour)
tri_c = (58, 28, 104, 255)
h = 192
pts = [(cx - h * 0.42, cy - h * 0.55), (cx + h * 0.62, cy), (cx - h * 0.42, cy + h * 0.55)]
tri = Image.new("RGBA", (S, S), (0, 0, 0, 0))
td = ImageDraw.Draw(tri)
td.polygon(pts, fill=tri_c)
td.line(pts + [pts[0]], fill=tri_c, width=34, joint="curve")
for p in pts:
    td.ellipse((p[0] - 17, p[1] - 17, p[0] + 17, p[1] + 17), fill=tri_c)
shadow = Image.new("RGBA", (S, S), (0, 0, 0, 0))
shadow.paste((120, 40, 20, 90), (0, 0), tri.split()[3])
canvas = Image.alpha_composite(canvas, ImageChops.offset(shadow, 0, 10).filter(ImageFilter.GaussianBlur(10)))
canvas = Image.alpha_composite(canvas, tri)

# --- water ---------------------------------------------------------------------------------
water = Image.new("RGBA", (S, S), (0, 0, 0, 0))
wd = ImageDraw.Draw(water)
wd.rectangle((0, 806, S, S), fill=(40, 26, 92, 130))
for y, w, a in [(828, 560, 120), (872, 700, 90), (918, 820, 60)]:
    wd.arc((512 - w // 2, y - 20, 512 + w // 2, y + 20), 200, 340, fill=(255, 220, 230, a), width=8)
wmask = tile_mask.copy()
water_clipped = Image.new("RGBA", (S, S), (0, 0, 0, 0))
water_clipped.paste(water, (0, 0), ImageChops.multiply(water.split()[3], wmask))
canvas = Image.alpha_composite(canvas, water_clipped)


# --- shapla (water lily) -------------------------------------------------------------------
def petal(base, angle_deg, length, width, top, bottom, edge):
    """Pointed petal growing from `base` at `angle_deg` (0 = straight up)."""
    pts_l, pts_r = [], []
    n = 60
    for i in range(n + 1):
        t = i / n
        half = width / 2 * math.sin(math.pi * t ** 0.9) ** 0.8
        pts_l.append((-half, -length * t))
        pts_r.append((half, -length * t))
    poly = pts_l + pts_r[::-1]
    a = math.radians(angle_deg)
    rot = [(base[0] + x * math.cos(a) - y * math.sin(a), base[1] + x * math.sin(a) + y * math.cos(a)) for x, y in poly]
    mask = Image.new("L", (S, S), 0)
    ImageDraw.Draw(mask).polygon(rot, fill=255)
    # gradient along the petal: approximate with a vertical gradient over its bounding box
    xs, ys = [p[0] for p in rot], [p[1] for p in rot]
    y0, y1 = int(min(ys)), int(max(ys)) + 1
    grad = vgradient(S, max(2, y1 - y0), [(0.0, top), (1.0, bottom)])
    fill = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    fill.paste(grad, (0, y0))
    layer = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    layer.paste(fill, (0, 0), mask)
    # soft outline + centre vein
    ld = ImageDraw.Draw(layer)
    ld.line(rot + [rot[0]], fill=edge, width=5, joint="curve")
    tip = (base[0] - length * 0.82 * math.sin(-a), base[1] - length * 0.82 * math.cos(a))
    ld.line([base, tip], fill=edge[:3] + (70,), width=4)
    return layer


base = (512, 850)
back_top, back_bot = (236, 98, 150, 255), (250, 170, 196, 255)
front_top, front_bot = (255, 236, 242, 255), (255, 255, 255, 255)
lily = Image.new("RGBA", (S, S), (0, 0, 0, 0))
for ang, ln, wd_ in [(-76, 270, 150), (76, 270, 150), (-52, 300, 165), (52, 300, 165)]:
    lily = Image.alpha_composite(lily, petal(base, ang, ln, wd_, back_top, back_bot, (190, 60, 110, 200)))
for ang, ln, wd_ in [(-26, 320, 170), (26, 320, 170)]:
    lily = Image.alpha_composite(lily, petal(base, ang, ln, wd_, (255, 210, 226, 255), front_bot, (214, 90, 140, 200)))
lily = Image.alpha_composite(lily, petal(base, 0, 335, 180, front_top, front_bot, (214, 90, 140, 210)))
lshadow = Image.new("RGBA", (S, S), (0, 0, 0, 0))
lshadow.paste((40, 10, 50, 120), (0, 0), lily.split()[3])
canvas = Image.alpha_composite(canvas, ImageChops.offset(lshadow, 0, 14).filter(ImageFilter.GaussianBlur(16)))
canvas = Image.alpha_composite(canvas, lily)

# --- tile edge highlight, then clip everything to the tile ---------------------------------
edge = Image.new("RGBA", (S, S), (0, 0, 0, 0))
ImageDraw.Draw(edge).rounded_rectangle((M + 3, M + 3, S - M - 3, S - M - 3), radius=226, outline=(255, 255, 255, 46), width=6)
canvas = Image.alpha_composite(canvas, edge)
final = Image.new("RGBA", (S, S), (0, 0, 0, 0))
final.paste(canvas, (0, 0), ImageChops.multiply(canvas.split()[3], tile_mask))

os.makedirs(out, exist_ok=True)
sizes = [16, 20, 24, 32, 40, 48, 64, 128, 256]
final.resize((256, 256), Image.LANCZOS).save(os.path.join(out, "icon-256.png"))
final.save(os.path.join(out, "icon-1024.png"))
final.save(os.path.join(out, "icon.ico"), sizes=[(s, s) for s in sizes])
# numeric sanity: opaque coverage and colour spread per size
for s in (16, 32, 256):
    im = final.resize((s, s), Image.LANCZOS)
    a = im.split()[3]
    cov = sum(1 for v in a.getdata() if v > 200) / (s * s)
    print(f"{s}px opaque={cov:.2f} colors={len(set(im.getdata()))}")
print("wrote", out)
