from PIL import Image, ImageDraw, ImageFilter
from pathlib import Path

# ====== SETTINGS ======
input_path = Path("icon.png")
output_path = Path("icon_rounded.png")

corner_radius = 200   # Increase for rounder corners
smoothness = 2      # Increase for softer/smoother edge blur
# ======================

img = Image.open(input_path).convert("RGBA")
width, height = img.size

# Create a rounded rectangle mask
mask = Image.new("L", (width, height), 0)
draw = ImageDraw.Draw(mask)

draw.rounded_rectangle(
    [(0, 0), (width, height)],
    radius=corner_radius,
    fill=255
)

# Slightly blur the mask edge for smoother anti-aliased corners
if smoothness > 0:
    mask = mask.filter(ImageFilter.GaussianBlur(smoothness))

# Apply mask as alpha channel
rounded = Image.new("RGBA", (width, height), (0, 0, 0, 0))
rounded.paste(img, (0, 0), mask)

rounded.save(output_path, "PNG")

print(f"Saved rounded image as: {output_path}")