"""Rebuild crisp MaxSpeech icons from a high-res master."""
from __future__ import annotations

import io
import struct
from pathlib import Path

from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parents[1]
SRC = Path(
    r"C:\Users\Paul Dimov\.cursor\projects\c-Users-Paul-Dimov-Documents-max-speech-project"
    r"\assets\maxspeech-icon-1024.jpg"
)
ICONS = ROOT / "src-tauri" / "icons"
PUBLIC = ROOT / "public"
WEBSITE = ROOT / "website"


def rounded_mask(size: int, radius: int) -> Image.Image:
    mask = Image.new("L", (size, size), 0)
    draw = ImageDraw.Draw(mask)
    draw.rounded_rectangle((0, 0, size - 1, size - 1), radius=radius, fill=255)
    return mask


def save_size(path: Path, master: Image.Image, size: int, rounded: bool = False) -> None:
    im = master.resize((size, size), Image.Resampling.LANCZOS)
    if rounded:
        im = im.copy()
        im.putalpha(rounded_mask(size, max(2, size // 6)))
    im.save(path, "PNG")
    print(f"{path.relative_to(ROOT)} {im.size} {path.stat().st_size}b")


def write_ico(path: Path, master: Image.Image, sizes: list[int]) -> None:
    entries: list[tuple[int, bytes]] = []
    for size in sizes:
        buf = io.BytesIO()
        master.resize((size, size), Image.Resampling.LANCZOS).save(buf, format="PNG")
        entries.append((size, buf.getvalue()))

    offset = 6 + 16 * len(entries)
    header = struct.pack("<HHH", 0, 1, len(entries))
    directory = b""
    blobs = b""
    for size, data in entries:
        width = 0 if size >= 256 else size
        height = 0 if size >= 256 else size
        directory += struct.pack("<BBBBHHII", width, height, 0, 0, 1, 32, len(data), offset)
        blobs += data
        offset += len(data)
    path.write_bytes(header + directory + blobs)
    print(f"{path.relative_to(ROOT)} {path.stat().st_size}b entries={len(entries)}")


def main() -> None:
    img = Image.open(SRC).convert("RGBA")
    width, height = img.size
    side = min(width, height)
    left = (width - side) // 2
    top = (height - side) // 2
    master = img.crop((left, top, left + side, top + side)).resize(
        (1024, 1024), Image.Resampling.LANCZOS
    )

    ICONS.mkdir(parents=True, exist_ok=True)
    master_path = ICONS / "icon-1024.png"
    master.save(master_path, "PNG")
    print(f"master {master_path} {master.size}")

    save_size(ICONS / "32x32.png", master, 32)
    save_size(ICONS / "128x128.png", master, 128)
    save_size(ICONS / "128x128@2x.png", master, 256)
    save_size(ICONS / "icon.png", master, 512)
    save_size(PUBLIC / "logo.png", master, 128, rounded=True)
    save_size(PUBLIC / "logo@2x.png", master, 256, rounded=True)
    save_size(WEBSITE / "logo.png", master, 128, rounded=True)
    save_size(WEBSITE / "logo@2x.png", master, 256, rounded=True)
    write_ico(ICONS / "icon.ico", master, [16, 24, 32, 48, 64, 128, 256])


if __name__ == "__main__":
    main()
