"""Build Windows-compatible multi-size ICO (embedded BMPs) + UI logos.

- Zooms ~15% into the center so the mark reads larger
- Rounds UI logos (public PNGs) with soft corners
- ICO/tray sizes: zoomed + light rounding that stays sharp at 16–32px
  (BMP-in-ICO avoids RC2176 / PNG-in-ICO installer blur)
"""
from __future__ import annotations

import struct
from pathlib import Path
from PIL import Image, ImageDraw

SRC = Path(r"C:\Users\Paul Dimov\Downloads\Max Speech icon.png")
OUT = Path(r"C:\Users\Paul Dimov\Projects\maxspeech\src-tauri\icons")
PUBLIC = Path(r"C:\Users\Paul Dimov\Projects\maxspeech\public")
SIZES = [16, 24, 32, 48, 64, 128, 256]
ZOOM = 1.15  # 15% more zoomed-in


def square_rgba(path: Path) -> Image.Image:
    im = Image.open(path).convert("RGBA")
    w, h = im.size
    side = min(w, h)
    left = (w - side) // 2
    top = (h - side) // 2
    return im.crop((left, top, left + side, top + side))


def zoom_center(im: Image.Image, zoom: float = ZOOM) -> Image.Image:
    """Crop into the center so the mark fills more of the square, then keep size."""
    if zoom <= 1.0:
        return im
    w, h = im.size
    crop_frac = 1.0 / zoom
    cw = max(1, int(round(w * crop_frac)))
    ch = max(1, int(round(h * crop_frac)))
    left = (w - cw) // 2
    top = (h - ch) // 2
    cropped = im.crop((left, top, left + cw, top + ch))
    return cropped.resize((w, h), Image.Resampling.LANCZOS)


def round_corners(im: Image.Image, radius_ratio: float) -> Image.Image:
    """Apply rounded-rect alpha mask. radius_ratio is fraction of half-side."""
    w, h = im.size
    radius = max(1, int(round(min(w, h) * 0.5 * radius_ratio)))
    # Soften tiny sizes so 16px isn't a weird pill
    if w <= 24:
        radius = max(1, int(round(w * 0.18)))
    elif w <= 48:
        radius = max(2, int(round(w * 0.22)))

    mask = Image.new("L", (w, h), 0)
    draw = ImageDraw.Draw(mask)
    draw.rounded_rectangle((0, 0, w - 1, h - 1), radius=radius, fill=255)

    out = im.copy()
    # Preserve existing transparency while clipping corners
    alpha = out.getchannel("A")
    alpha = Image.composite(alpha, Image.new("L", (w, h), 0), mask)
    out.putalpha(alpha)
    return out


def rgba_to_bmp_icon(img: Image.Image) -> bytes:
    """32-bit BGRA BMP (no file header) for ICO — bottom-up, AND mask after."""
    w, h = img.size
    pixels = img.load()
    xor = bytearray()
    for y in range(h - 1, -1, -1):
        for x in range(w):
            r, g, b, a = pixels[x, y]
            xor += bytes((b, g, r, a))
    # AND mask: 1 bit per pixel, padded to 32-bit rows.
    # For 32-bpp icons Windows uses alpha; keep AND zeros (fully visible).
    row_bytes = ((w + 31) // 32) * 4
    and_mask = bytearray(row_bytes * h)

    dib = struct.pack(
        "<IiiHHIIiiII",
        40,  # header size
        w,
        h * 2,  # height includes AND mask
        1,  # planes
        32,  # bpp
        0,  # compression
        len(xor) + len(and_mask),
        0,
        0,
        0,
        0,
    )
    return dib + bytes(xor) + bytes(and_mask)


def write_ico(path: Path, images: list[Image.Image]) -> None:
    entries = []
    blobs = []
    offset = 6 + 16 * len(images)
    for im in images:
        blob = rgba_to_bmp_icon(im)
        w, h = im.size
        entries.append(
            struct.pack(
                "<BBBBHHII",
                w if w < 256 else 0,
                h if h < 256 else 0,
                0,  # colors
                0,  # reserved
                1,  # planes
                32,  # bit count
                len(blob),
                offset,
            )
        )
        blobs.append(blob)
        offset += len(blob)

    header = struct.pack("<HHH", 0, 1, len(images))
    path.write_bytes(header + b"".join(entries) + b"".join(blobs))


def main() -> None:
    if not SRC.is_file():
        raise SystemExit(f"Source icon not found: {SRC}")

    PUBLIC.mkdir(exist_ok=True)
    OUT.mkdir(exist_ok=True)

    base = zoom_center(square_rgba(SRC), ZOOM)

    def resize(n: int) -> Image.Image:
        return base.resize((n, n), Image.Resampling.LANCZOS)

    # System / installer / tray — zoomed + light rounding (still fills the tile)
    def system_icon(n: int) -> Image.Image:
        return round_corners(resize(n), radius_ratio=0.28)

    # UI logos — stronger rounded edges for in-app display
    def ui_logo(n: int) -> Image.Image:
        return round_corners(resize(n), radius_ratio=0.36)

    system_icon(32).save(OUT / "32x32.png", optimize=True)
    system_icon(128).save(OUT / "128x128.png", optimize=True)
    system_icon(256).save(OUT / "128x128@2x.png", optimize=True)
    system_icon(256).save(OUT / "icon.png", optimize=True)

    ui_logo(128).save(PUBLIC / "logo.png", optimize=True)
    ui_logo(256).save(PUBLIC / "logo@2x.png", optimize=True)

    images = [system_icon(s) for s in SIZES]
    write_ico(OUT / "icon.ico", images)

    raw = (OUT / "icon.ico").read_bytes()
    count = struct.unpack_from("<H", raw, 4)[0]
    print(f"ICO entries: {count}, size={len(raw)} bytes")
    for i in range(count):
        w, h, _, _, _, _, size, off = struct.unpack_from("<BBBBHHII", raw, 6 + i * 16)
        print(f"  #{i}: {w or 256}x{h or 256} blob={size} @ {off}")
    print("done")


if __name__ == "__main__":
    main()
