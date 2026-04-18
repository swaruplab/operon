"""
Generate 5 high-impact, professional YouTube thumbnails (1280x720)
for the Operon tutorial video series.
"""

from PIL import Image, ImageDraw, ImageFont, ImageFilter
import math
import os
import random

W, H = 1280, 720
OUT_DIR = "/sessions/trusting-hopeful-feynman/mnt/docs/operon_thumbnails"
os.makedirs(OUT_DIR, exist_ok=True)

# ---------- Font loader ----------
FONT_CANDIDATES_BOLD = [
    "/System/Library/Fonts/Supplemental/Arial Black.ttf",
    "/System/Library/Fonts/Helvetica.ttc",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
]
FONT_CANDIDATES_REG = [
    "/System/Library/Fonts/Helvetica.ttc",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
]

def load_font(size, bold=True):
    candidates = FONT_CANDIDATES_BOLD if bold else FONT_CANDIDATES_REG
    for p in candidates:
        if os.path.exists(p):
            try:
                return ImageFont.truetype(p, size)
            except Exception:
                continue
    return ImageFont.load_default()


# ---------- Helpers ----------
def vertical_gradient(size, top_color, bottom_color):
    w, h = size
    base = Image.new("RGB", (w, h), top_color)
    top = Image.new("RGB", (w, h), top_color)
    bot = Image.new("RGB", (w, h), bottom_color)
    mask = Image.new("L", (w, h))
    for y in range(h):
        mask.paste(int(255 * y / h), (0, y, w, y + 1))
    base = Image.composite(bot, top, mask)
    return base


def diagonal_gradient(size, c1, c2, angle=135):
    w, h = size
    img = Image.new("RGB", (w, h), c1)
    draw = ImageDraw.Draw(img)
    rad = math.radians(angle)
    dx, dy = math.cos(rad), math.sin(rad)
    diag = abs(w * dx) + abs(h * dy)
    for i in range(int(diag)):
        t = i / diag
        r = int(c1[0] * (1 - t) + c2[0] * t)
        g = int(c1[1] * (1 - t) + c2[1] * t)
        b = int(c1[2] * (1 - t) + c2[2] * t)
        # draw a line perpendicular to direction
        x0 = i * dx
        y0 = i * dy
        draw.line(
            [(x0 - h * dy, y0 + h * dx), (x0 + h * dy, y0 - h * dx)],
            fill=(r, g, b),
            width=2,
        )
    return img


def draw_text_with_shadow(draw, xy, text, font, fill, shadow=(0, 0, 0, 200), offset=(4, 5), shadow_blur=False):
    x, y = xy
    sx, sy = offset
    draw.text((x + sx, y + sy), text, font=font, fill=shadow)
    draw.text((x, y), text, font=font, fill=fill)


def text_size(draw, text, font):
    bbox = draw.textbbox((0, 0), text, font=font)
    return bbox[2] - bbox[0], bbox[3] - bbox[1]


def rounded_rect(draw, xy, radius, fill=None, outline=None, width=1):
    x0, y0, x1, y1 = xy
    draw.rounded_rectangle(xy, radius=radius, fill=fill, outline=outline, width=width)


def add_noise(img, amount=8):
    """Subtle film grain for premium look."""
    px = img.load()
    w, h = img.size
    for _ in range(int(w * h * 0.02)):
        x = random.randint(0, w - 1)
        y = random.randint(0, h - 1)
        r, g, b = px[x, y][:3]
        n = random.randint(-amount, amount)
        px[x, y] = (
            max(0, min(255, r + n)),
            max(0, min(255, g + n)),
            max(0, min(255, b + n)),
        )
    return img


def draw_glow_circle(img, center, radius, color, blur=60):
    layer = Image.new("RGBA", img.size, (0, 0, 0, 0))
    d = ImageDraw.Draw(layer)
    cx, cy = center
    d.ellipse(
        [cx - radius, cy - radius, cx + radius, cy + radius],
        fill=color,
    )
    layer = layer.filter(ImageFilter.GaussianBlur(blur))
    img.alpha_composite(layer)


def draw_operon_badge(img, position="top-left"):
    """Brand badge: OPERON tag."""
    draw = ImageDraw.Draw(img, "RGBA")
    badge_font = load_font(34, bold=True)
    tag = "OPERON"
    pad_x, pad_y = 22, 10
    tw, th = text_size(draw, tag, badge_font)
    if position == "top-left":
        x0, y0 = 40, 40
    else:
        x0, y0 = W - tw - pad_x * 2 - 40, 40
    rect = (x0, y0, x0 + tw + pad_x * 2, y0 + th + pad_y * 2)
    draw.rounded_rectangle(rect, radius=10, fill=(255, 255, 255, 230))
    draw.text((x0 + pad_x, y0 + pad_y - 4), tag, font=badge_font, fill=(15, 20, 35))
    # accent dot
    draw.ellipse(
        [rect[2] + 10, rect[1] + (rect[3] - rect[1]) / 2 - 6,
         rect[2] + 22, rect[1] + (rect[3] - rect[1]) / 2 + 6],
        fill=(255, 200, 50, 255),
    )


def wrap_lines(text, max_chars):
    words = text.split()
    lines, cur = [], ""
    for w in words:
        if len(cur) + len(w) + 1 <= max_chars:
            cur = (cur + " " + w).strip()
        else:
            lines.append(cur)
            cur = w
    if cur:
        lines.append(cur)
    return lines


def draw_title_block(img, title_lines, subtitle, accent_color, base_y=240):
    draw = ImageDraw.Draw(img, "RGBA")
    title_font = load_font(110, bold=True)
    sub_font = load_font(42, bold=True)

    # Accent bar
    draw.rectangle([60, base_y, 78, base_y + len(title_lines) * 130 + 60], fill=accent_color)

    y = base_y - 10
    for line in title_lines:
        # Big text with strong shadow
        x = 110
        # outline-style shadow for premium look
        shadow_color = (0, 0, 0, 220)
        for dx, dy in [(5, 6)]:
            draw.text((x + dx, y + dy), line, font=title_font, fill=shadow_color)
        draw.text((x, y), line, font=title_font, fill=(255, 255, 255, 255))
        y += 125

    # Subtitle pill
    if subtitle:
        sw, sh = text_size(draw, subtitle, sub_font)
        sx0, sy0 = 110, y + 20
        rect = (sx0 - 18, sy0 - 8, sx0 + sw + 22, sy0 + sh + 16)
        draw.rounded_rectangle(rect, radius=14, fill=accent_color)
        draw.text((sx0, sy0), subtitle, font=sub_font, fill=(15, 20, 35))


# =======================================================================
# THUMBNAIL 1 — macOS Installation
# =======================================================================
def thumb_macos():
    img = vertical_gradient((W, H), (10, 12, 24), (40, 48, 80)).convert("RGBA")

    # Glow accents
    draw_glow_circle(img, (1050, 220), 320, (120, 200, 255, 90), blur=80)
    draw_glow_circle(img, (1100, 540), 220, (80, 150, 255, 70), blur=100)

    # Decorative grid lines
    overlay = Image.new("RGBA", img.size, (0, 0, 0, 0))
    od = ImageDraw.Draw(overlay)
    for i in range(0, W, 80):
        od.line([(i, 0), (i, H)], fill=(255, 255, 255, 12), width=1)
    for j in range(0, H, 80):
        od.line([(0, j), (W, j)], fill=(255, 255, 255, 12), width=1)
    img.alpha_composite(overlay)

    # Laptop / macOS window mockup
    lp = Image.new("RGBA", img.size, (0, 0, 0, 0))
    ld = ImageDraw.Draw(lp)
    cx, cy = 1010, 380
    # screen
    sw, sh = 380, 240
    ld.rounded_rectangle([cx - sw // 2, cy - sh // 2, cx + sw // 2, cy + sh // 2],
                         radius=18, fill=(15, 18, 32, 255),
                         outline=(180, 195, 230, 255), width=4)
    # title bar
    ld.rounded_rectangle([cx - sw // 2 + 4, cy - sh // 2 + 4, cx + sw // 2 - 4, cy - sh // 2 + 36],
                         radius=14, fill=(35, 40, 60, 255))
    for i, c in enumerate([(255, 95, 86), (255, 189, 46), (39, 201, 63)]):
        ld.ellipse([cx - sw // 2 + 18 + i * 24, cy - sh // 2 + 12,
                    cx - sw // 2 + 32 + i * 24, cy - sh // 2 + 26], fill=c)
    # base / stand
    ld.rounded_rectangle([cx - sw // 2 - 30, cy + sh // 2 + 2, cx + sw // 2 + 30, cy + sh // 2 + 18],
                         radius=4, fill=(140, 155, 195, 255))
    ld.rectangle([cx - 80, cy + sh // 2 + 18, cx + 80, cy + sh // 2 + 28],
                 fill=(110, 125, 165, 255))

    # Download arrow inside the window
    ax, ay = cx, cy + 16
    ld.polygon(
        [(ax - 50, ay - 60), (ax + 50, ay - 60), (ax + 50, ay - 10),
         (ax + 86, ay - 10), (ax, ay + 78), (ax - 86, ay - 10),
         (ax - 50, ay - 10)],
        fill=(0, 145, 255, 255),
    )
    # progress bar under arrow
    ld.rounded_rectangle([cx - 130, cy + sh // 2 - 30, cx + 130, cy + sh // 2 - 14],
                         radius=8, fill=(60, 70, 100, 255))
    ld.rounded_rectangle([cx - 130, cy + sh // 2 - 30, cx + 60, cy + sh // 2 - 14],
                         radius=8, fill=(0, 200, 255, 255))
    img.alpha_composite(lp)

    draw_operon_badge(img)

    draw_title_block(
        img,
        ["INSTALL", "OPERON ON", "MACOS"],
        "STEP-BY-STEP TUTORIAL",
        accent_color=(0, 200, 255, 255),
        base_y=180,
    )

    # Bottom corner: ".PKG" file tag
    draw = ImageDraw.Draw(img, "RGBA")
    tag_font = load_font(28, bold=True)
    draw.rounded_rectangle((W - 220, H - 90, W - 50, H - 40), radius=10, fill=(255, 255, 255, 230))
    draw.text((W - 200, H - 82), "macOS  •  .DMG", font=tag_font, fill=(15, 20, 35))

    img = add_noise(img.convert("RGB"), amount=5)
    img.save(os.path.join(OUT_DIR, "01_macos_installation.png"), "PNG", quality=95)
    print("Saved 01_macos_installation.png")


# =======================================================================
# THUMBNAIL 2 — Remote Server Connection
# =======================================================================
def thumb_remote_server():
    img = vertical_gradient((W, H), (5, 30, 60), (10, 70, 130)).convert("RGBA")

    # Glow
    draw_glow_circle(img, (1000, 360), 360, (50, 200, 255, 110), blur=90)

    # Network lines
    nl = Image.new("RGBA", img.size, (0, 0, 0, 0))
    nd = ImageDraw.Draw(nl)
    nodes = [(870, 180), (1180, 250), (1100, 480), (920, 580), (760, 380)]
    for a in nodes:
        for b in nodes:
            if a != b:
                nd.line([a, b], fill=(120, 220, 255, 70), width=2)
    for n in nodes:
        nd.ellipse([n[0] - 14, n[1] - 14, n[0] + 14, n[1] + 14], fill=(255, 255, 255, 230))
        nd.ellipse([n[0] - 22, n[1] - 22, n[0] + 22, n[1] + 22], outline=(120, 220, 255, 200), width=3)
    img.alpha_composite(nl)

    # Server stack icon
    sl = Image.new("RGBA", img.size, (0, 0, 0, 0))
    sd = ImageDraw.Draw(sl)
    sx, sy = 940, 380
    for i in range(3):
        y = sy - 100 + i * 80
        sd.rounded_rectangle([sx - 110, y, sx + 110, y + 60], radius=10,
                             fill=(20, 40, 80, 240), outline=(120, 220, 255, 255), width=3)
        # status LED
        sd.ellipse([sx + 80, y + 22, sx + 100, y + 42], fill=(50, 255, 130, 255))
        # bars
        for k in range(4):
            sd.rectangle([sx - 90 + k * 12, y + 24, sx - 84 + k * 12, y + 36],
                         fill=(120, 220, 255, 220))
    img.alpha_composite(sl)

    draw_operon_badge(img)

    draw_title_block(
        img,
        ["CONNECT TO", "REMOTE", "SERVER"],
        "OPERON SSH GUIDE",
        accent_color=(80, 220, 255, 255),
        base_y=170,
    )

    img = add_noise(img.convert("RGB"), amount=5)
    img.save(os.path.join(OUT_DIR, "02_remote_server_connection.png"), "PNG", quality=95)
    print("Saved 02_remote_server_connection.png")


# =======================================================================
# THUMBNAIL 3 — Claude Installation on Remote Server
# =======================================================================
def thumb_claude_install():
    img = vertical_gradient((W, H), (35, 15, 5), (90, 40, 15)).convert("RGBA")

    draw_glow_circle(img, (1020, 360), 340, (255, 150, 80, 130), blur=90)

    # Terminal-style code block on right
    tl = Image.new("RGBA", img.size, (0, 0, 0, 0))
    td = ImageDraw.Draw(tl)
    tx0, ty0, tx1, ty1 = 770, 200, 1230, 580
    td.rounded_rectangle([tx0, ty0, tx1, ty1], radius=18, fill=(20, 18, 30, 240),
                         outline=(255, 140, 80, 255), width=3)
    # window dots
    for i, c in enumerate([(255, 95, 86), (255, 189, 46), (39, 201, 63)]):
        td.ellipse([tx0 + 22 + i * 28, ty0 + 22, tx0 + 38 + i * 28, ty0 + 38], fill=c)

    code_font = load_font(26, bold=True)
    code_lines = [
        "$ ssh user@server",
        "$ curl -fsSL claude.ai",
        "  /install.sh | sh",
        "",
        "✓ Claude installed",
        "✓ Ready to launch",
    ]
    cy = ty0 + 70
    for line in code_lines:
        color = (255, 200, 130) if line.startswith("$") else (
            (130, 255, 170) if line.startswith("✓") else (220, 220, 230))
        td.text((tx0 + 24, cy), line, font=code_font, fill=color)
        cy += 38
    img.alpha_composite(tl)

    # Sparkle / AI accent
    sp = Image.new("RGBA", img.size, (0, 0, 0, 0))
    spd = ImageDraw.Draw(sp)
    cx, cy = tx1 - 60, ty0 + 60
    for r, a in [(40, 60), (24, 140), (10, 240)]:
        spd.ellipse([cx - r, cy - r, cx + r, cy + r], fill=(255, 180, 90, a))
    img.alpha_composite(sp)

    draw_operon_badge(img)

    draw_title_block(
        img,
        ["INSTALL", "CLAUDE ON", "REMOTE"],
        "OPERON + AI SETUP",
        accent_color=(255, 165, 80, 255),
        base_y=170,
    )

    img = add_noise(img.convert("RGB"), amount=5)
    img.save(os.path.join(OUT_DIR, "03_claude_remote_install.png"), "PNG", quality=95)
    print("Saved 03_claude_remote_install.png")


# =======================================================================
# THUMBNAIL 4 — scRNA-seq Scanpy
# =======================================================================
def thumb_scanpy():
    img = vertical_gradient((W, H), (20, 5, 40), (70, 20, 110)).convert("RGBA")

    # UMAP-like scatter cluster
    sl = Image.new("RGBA", img.size, (0, 0, 0, 0))
    sd = ImageDraw.Draw(sl)
    random.seed(7)
    cluster_centers = [
        (970, 260, (255, 90, 140)),
        (1100, 380, (120, 200, 255)),
        (880, 440, (255, 200, 90)),
        (1020, 540, (130, 255, 170)),
        (820, 320, (200, 130, 255)),
    ]
    for cx, cy, color in cluster_centers:
        for _ in range(160):
            dx = int(random.gauss(0, 38))
            dy = int(random.gauss(0, 38))
            r = random.randint(3, 6)
            sd.ellipse(
                [cx + dx - r, cy + dy - r, cx + dx + r, cy + dy + r],
                fill=color + (220,),
            )
    img.alpha_composite(sl)

    # Dashed circle highlighting a cluster
    hl = Image.new("RGBA", img.size, (0, 0, 0, 0))
    hd = ImageDraw.Draw(hl)
    cx, cy, rad = 970, 260, 95
    steps = 40
    for i in range(steps):
        if i % 2 == 0:
            a0 = i * 360 / steps
            a1 = (i + 1) * 360 / steps
            hd.arc([cx - rad, cy - rad, cx + rad, cy + rad],
                   start=a0, end=a1, fill=(255, 255, 255, 240), width=4)
    img.alpha_composite(hl)

    draw_operon_badge(img)

    draw_title_block(
        img,
        ["scRNA-seq", "WITH", "SCANPY"],
        "SINGLE-CELL ANALYSIS",
        accent_color=(255, 110, 180, 255),
        base_y=170,
    )

    # Tag: Python
    draw = ImageDraw.Draw(img, "RGBA")
    tag_font = load_font(28, bold=True)
    draw.rounded_rectangle((W - 240, H - 90, W - 50, H - 40), radius=10, fill=(255, 255, 255, 230))
    draw.text((W - 222, H - 82), "PYTHON  •  SCANPY", font=tag_font, fill=(40, 10, 70))

    img = add_noise(img.convert("RGB"), amount=5)
    img.save(os.path.join(OUT_DIR, "04_scanpy_scRNAseq.png"), "PNG", quality=95)
    print("Saved 04_scanpy_scRNAseq.png")


# =======================================================================
# THUMBNAIL 5 — CellPose Segmentation
# =======================================================================
def thumb_cellpose():
    img = vertical_gradient((W, H), (5, 35, 35), (10, 80, 70)).convert("RGBA")

    draw_glow_circle(img, (1010, 360), 320, (80, 255, 200, 110), blur=90)

    # Cell segmentation visualization - irregular blobs
    cl = Image.new("RGBA", img.size, (0, 0, 0, 0))
    cd = ImageDraw.Draw(cl)
    random.seed(42)
    cell_palette = [
        (255, 150, 200, 220), (150, 220, 255, 220), (255, 220, 130, 220),
        (180, 255, 180, 220), (255, 180, 130, 220), (200, 170, 255, 220),
        (130, 255, 220, 220),
    ]
    centers = []
    attempts = 0
    while len(centers) < 14 and attempts < 400:
        cx = random.randint(770, 1230)
        cy = random.randint(180, 600)
        r = random.randint(45, 78)
        ok = True
        for (ex, ey, er) in centers:
            if math.hypot(cx - ex, cy - ey) < r + er - 10:
                ok = False
                break
        if ok:
            centers.append((cx, cy, r))
        attempts += 1

    for i, (cx, cy, r) in enumerate(centers):
        color = cell_palette[i % len(cell_palette)]
        # irregular polygon
        pts = []
        steps = 18
        for k in range(steps):
            ang = 2 * math.pi * k / steps
            jitter = random.uniform(0.78, 1.18)
            x = cx + math.cos(ang) * r * jitter
            y = cy + math.sin(ang) * r * jitter
            pts.append((x, y))
        cd.polygon(pts, fill=color, outline=(255, 255, 255, 255))
        # nucleus
        nr = int(r * 0.35)
        cd.ellipse([cx - nr, cy - nr, cx + nr, cy + nr],
                   fill=(40, 30, 80, 200))
    img.alpha_composite(cl)

    draw_operon_badge(img)

    draw_title_block(
        img,
        ["CELL", "SEGMENTATION", "WITH CELLPOSE"],
        "BIOIMAGE TUTORIAL",
        accent_color=(80, 255, 200, 255),
        base_y=140,
    )

    # Add a translucent strip behind the bottom tag so it stays legible
    draw = ImageDraw.Draw(img, "RGBA")
    tag_font = load_font(28, bold=True)
    label = "AI  •  DEEP LEARNING"
    tw, th = text_size(draw, label, tag_font)
    pad = 22
    box_x1 = W - 40
    box_x0 = box_x1 - tw - pad * 2
    box_y0, box_y1 = H - 90, H - 40
    # solid backing strip behind the box
    draw.rectangle((box_x0 - 20, box_y0 - 10, W, H), fill=(5, 35, 35, 220))
    draw.rounded_rectangle((box_x0, box_y0, box_x1, box_y1), radius=10, fill=(255, 255, 255, 240))
    draw.text((box_x0 + pad, box_y0 + (box_y1 - box_y0 - th) // 2 - 4),
              label, font=tag_font, fill=(5, 35, 35))

    img = add_noise(img.convert("RGB"), amount=5)
    img.save(os.path.join(OUT_DIR, "05_cellpose_segmentation.png"), "PNG", quality=95)
    print("Saved 05_cellpose_segmentation.png")


if __name__ == "__main__":
    thumb_macos()
    thumb_remote_server()
    thumb_claude_install()
    thumb_scanpy()
    thumb_cellpose()
    print("All thumbnails saved to:", OUT_DIR)
