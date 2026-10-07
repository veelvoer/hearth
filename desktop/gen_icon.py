#!/usr/bin/env python3
"""Renders the Hearth logo (../branding/*.svg) to the app icon and tray icon. Needs ImageMagick (development only)."""
import subprocess
from pathlib import Path

here = Path(__file__).parent
brand = here.parent / "branding"
out = here / "assets"
out.mkdir(exist_ok=True)
for svg, png, size in (("hearth.svg", "icon.png", 512), ("flame.svg", "tray.png", 32)):
    subprocess.run(["magick", "-background", "none", "-density", "384", str(brand / svg), "-resize", f"{size}x{size}", str(out / png)], check=True)
print("icons written")
