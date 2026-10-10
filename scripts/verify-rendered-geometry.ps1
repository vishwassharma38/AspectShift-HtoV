param(
  [string]$Ffmpeg = (Join-Path $PSScriptRoot "..\src-tauri\bin\ffmpeg-x86_64-pc-windows-msvc.exe")
)

# TEST-ONLY phase, Test 6.9 (renderer side): prove with real FFmpeg/libass
# renders that off-canvas overlay geometry is clipped by the video frame
# rather than repositioned, and that centered geometry stays centered.
#
# Method (same pattern as scripts/verify-text-style-matrix.ps1): write ASS
# files in the app's writer format (PlayRes = video size, \an5\pos center
# anchor), render them with the `ass` filter onto a black lavfi source, then
# measure the ink bounding box with deterministic LockBits pixel analysis
# (threshold 24; edge-touch tolerance 4px for antialiased tails).
# Assertions use generous geometric tolerances -- this is a geometry test,
# not a pixel test: Chrome/libass glyph differences must never fail it.
#
# What this proves (renderer-level, behavioral):
#   - signed off-canvas ASS coordinates render (no clamp, no wraparound)
#   - clipped side touches the frame edge, centered side does not move
#   - larger canonical fontSize yields a larger ink box (size semantics)
# What it does NOT prove: preview screenshots (no browser harness in repo).

$ErrorActionPreference = "Stop"
$repo = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$fonts = (Resolve-Path (Join-Path $repo "src-tauri\resources\fonts\text-overlay\fira-sans")).Path
$work = Join-Path ([System.IO.Path]::GetTempPath()) ("aspectshift-render-geometry-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $work | Out-Null
$renderFonts = Join-Path $work "fonts"
New-Item -ItemType Directory -Path $renderFonts | Out-Null
Copy-Item -LiteralPath (Join-Path $fonts "FiraSans-Regular.ttf") -Destination $renderFonts -Force
Copy-Item -LiteralPath (Join-Path $fonts "FiraSans-Bold.ttf") -Destination $renderFonts -Force

$W = 1080
$H = 1920

function Write-OverlayAss([string]$name, [string]$text, [int]$posX, [int]$posY, [int]$fontSize) {
  $ass = Join-Path $work ($name + ".ass")
  $content = @"
[Script Info]
ScriptType: v4.00+
PlayResX: $W
PlayResY: $H
ScaledBorderAndShadow: yes

[V4+ Styles]
Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding
Style: TextOverlay1,Fira Sans,$fontSize,&H00FFFFFF,&H000000FF,&H00000000,&HFF000000,0,0,0,0,100,100,0,0,1,0,0,5,0,0,0,1

[Events]
Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text
Dialogue: 0,0:00:00.00,0:00:02.00,TextOverlay1,,0,0,0,,{\an5\pos($posX,$posY)}$text
"@
  Set-Content -LiteralPath $ass -Value $content -Encoding utf8
  return $ass
}

function Measure-InkBox([string]$assPath) {
  $filterAss = $assPath.Replace('\', '/').Replace(':', '\:').Replace("'", "\'")
  $filterFonts = $renderFonts.Replace('\', '/').Replace(':', '\:').Replace("'", "\'")
  $png = Join-Path $work ([System.IO.Path]::GetFileNameWithoutExtension($assPath) + ".png")
  $ErrorActionPreference = "Continue"
  $renderLog = & $Ffmpeg -hide_banner -loglevel verbose -f lavfi -i "color=c=black:s=${W}x${H}:d=1" -vf "ass='$filterAss':fontsdir='$filterFonts'" -frames:v 1 -y $png 2>&1
  if ($LASTEXITCODE -ne 0) { throw ($renderLog -join [Environment]::NewLine) }
  if (-not (($renderLog -join "`n") -match "FiraSans-Regular.ttf")) { throw "libass did not select the bundled FiraSans face:`n$($renderLog -join [Environment]::NewLine)" }
  $ErrorActionPreference = "Stop"
  # Deterministic ink bounding box via LockBits (cropdetect inverts its x
  # measurements on narrow centered content with this build; pixel analysis
  # is fully under this script's control). Threshold 24 ignores conversion
  # noise; background is pure black, text is white.
  if (-not ([System.Management.Automation.PSTypeName]"InkBox").Type) {
    Add-Type -TypeDefinition @"
using System;
using System.Drawing;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;
public static class InkBox {
  public static string Measure(string path, int threshold) {
    using (var bmp = new Bitmap(path)) {
      var rect = new Rectangle(0, 0, bmp.Width, bmp.Height);
      var data = bmp.LockBits(rect, ImageLockMode.ReadOnly, PixelFormat.Format24bppRgb);
      try {
        int stride = Math.Abs(data.Stride);
        byte[] buf = new byte[stride * data.Height];
        Marshal.Copy(data.Scan0, buf, 0, buf.Length);
        int minX = bmp.Width, minY = bmp.Height, maxX = -1, maxY = -1;
        for (int y = 0; y < data.Height; y++) {
          int row = y * stride;
          for (int x = 0; x < bmp.Width; x++) {
            int i = row + x * 3;
            if (buf[i] > threshold || buf[i + 1] > threshold || buf[i + 2] > threshold) {
              if (x < minX) minX = x;
              if (x > maxX) maxX = x;
              if (y < minY) minY = y;
              if (y > maxY) maxY = y;
            }
          }
        }
        if (maxX < 0) return "empty";
        return string.Format("{0},{1},{2},{3}", minX, minY, maxX - minX + 1, maxY - minY + 1);
      } finally { bmp.UnlockBits(data); }
    }
  }
}
"@ -ReferencedAssemblies "System.Drawing"
  }
  $measured = [InkBox]::Measure($png, 24)
  if ($measured -eq "empty") { throw "rendered frame contains no ink for $assPath" }
  $parts = $measured.Split(",")
  return @{ x = [int]$parts[0]; y = [int]$parts[1]; w = [int]$parts[2]; h = [int]$parts[3] }
}

function Assert-True([bool]$condition, [string]$message) {
  if (-not $condition) { throw "ASSERT FAILED: $message" }
}

try {
  # Case 1 (Test 6.1 geometry): centered "I love this" at fontSize 96.
  $center = Measure-InkBox (Write-OverlayAss "center" "I love this" 540 960 96)
  Write-Output ("center box: w={0} h={1} x={2} y={3}" -f $center.w, $center.h, $center.x, $center.y)
  $centerX = $center.x + $center.w / 2
  Assert-True ([Math]::Abs($centerX - 540) -le 40) "centered text ink must be centered near x=540 (got $centerX)"
  Assert-True ($center.x -gt 0 -and ($center.x + $center.w) -lt $W) "centered text must be fully inside the frame"

  # Case 2 (Tests 6.2/6.9 representative): x=-0.15 -> pos(-162,960).
  # -0.15 * 1080 = -162: the same signed conversion the Rust ASS tests assert.
  # Edge-touch tolerance is 4px: antialiased glyph tails below the ink
  # threshold do not change the geometric conclusion (clipped width 13px
  # vs fully-visible width 345px).
  $left = Measure-InkBox (Write-OverlayAss "left" "I love this" -162 960 96)
  Write-Output ("left-clipped box: w={0} h={1} x={2} y={3}" -f $left.w, $left.h, $left.x, $left.y)
  Assert-True ($left.x -le 4) "off-canvas-left text must touch the left frame edge (clipped, got x=$($left.x))"
  Assert-True ($left.w -lt $center.w) "clipped text must be narrower than the fully visible text (got $($left.w) vs $($center.w))"
  # Clamp-regression discriminator: a backend that clamped -0.15 to 0 would
  # render the left half of the text (visible extent ~half width). True
  # off-canvas geometry leaves only a sliver near the edge.
  Assert-True (($left.x + $left.w) -lt 100) "off-canvas-left visible extent must stay near the edge (got $($left.x + $left.w))"

  # Case 3 (Test 6.3 geometry): x=1.1 -> pos(1188,960) on 1080 wide.
  $right = Measure-InkBox (Write-OverlayAss "right" "I love this" 1188 960 96)
  Write-Output ("right-clipped box: w={0} h={1} x={2} y={3}" -f $right.w, $right.h, $right.x, $right.y)
  Assert-True (($W - ($right.x + $right.w)) -le 4) "off-canvas-right text must touch the right frame edge (got $($right.x + $right.w) vs $W)"
  Assert-True ($right.w -lt $center.w) "clipped text must be narrower than the fully visible text"
  # Clamp-regression discriminator: a backend that clamped 1.1 to 1.0 would
  # leave the right half visible (starting near x=907). True off-canvas
  # geometry starts within a sliver of the right edge.
  Assert-True ($right.x -gt ($W - 100)) "off-canvas-right visible start must stay near the edge (got $($right.x))"

  # Case 4 (size semantics): same text/position at fontSize 48 vs 96.
  $small = Measure-InkBox (Write-OverlayAss "small" "I love this" 540 960 48)
  Assert-True ($center.w -gt $small.w -and $center.h -gt $small.h) "larger canonical fontSize must yield a larger rendered ink box"

  Write-Output "PASS: rendered-geometry clipping/center/size verified with real libass output."
}
finally {
  if (Test-Path -LiteralPath $work) {
    $resolvedWork = (Resolve-Path -LiteralPath $work).Path
    $resolvedTemp = (Resolve-Path -LiteralPath ([System.IO.Path]::GetTempPath())).Path
    if (-not $resolvedWork.StartsWith($resolvedTemp, [StringComparison]::OrdinalIgnoreCase)) {
      throw "Refusing to clean a test directory outside the system temp directory: $resolvedWork"
    }
    Remove-Item -LiteralPath $resolvedWork -Recurse -Force
  }
}
