param(
  [string]$Ffmpeg = (Join-Path $PSScriptRoot "..\src-tauri\bin\ffmpeg-x86_64-pc-windows-msvc.exe")
)

$ErrorActionPreference = "Stop"
$repo = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$fonts = (Resolve-Path (Join-Path $repo "src-tauri\resources\fonts\text-overlay")).Path
$work = Join-Path ([System.IO.Path]::GetTempPath()) ("aspectshift-font-matrix-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $work | Out-Null
$renderFonts = Join-Path $work "fonts"
New-Item -ItemType Directory -Path $renderFonts | Out-Null

$families = @(
  @{ Style = "clean"; Name = "Fira Sans"; Dir = "fira-sans"; Prefix = "FiraSans" },
  @{ Style = "minimal"; Name = "Lato"; Dir = "lato"; Prefix = "Lato" },
  @{ Style = "caption"; Name = "Inter"; Dir = "inter"; Prefix = "Inter" },
  @{ Style = "meme"; Name = "Anton"; Dir = "anton"; Prefix = "Anton" },
  @{ Style = "creator"; Name = "Montserrat"; Dir = "montserrat"; Prefix = "Montserrat" },
  @{ Style = "gaming"; Name = "Exo 2"; Dir = "exo-2"; Prefix = "Exo2" },
  @{ Style = "cyberpunk"; Name = "Orbitron"; Dir = "orbitron"; Prefix = "Orbitron" },
  @{ Style = "cinematic"; Name = "Cormorant Garamond"; Dir = "cormorant-garamond"; Prefix = "CormorantGaramond" },
  @{ Style = "retro"; Name = "Bungee"; Dir = "bungee"; Prefix = "Bungee" },
  @{ Style = "handwritten"; Name = "Caveat"; Dir = "caveat"; Prefix = "Caveat" }
)
$faces = @(
  @{ Name = "regular"; Suffix = "Regular"; Bold = 0; Italic = 0 },
  @{ Name = "bold"; Suffix = "Bold"; Bold = -1; Italic = 0 },
  @{ Name = "italic"; Suffix = "Italic"; Bold = 0; Italic = -1 },
  @{ Name = "bold-italic"; Suffix = "BoldItalic"; Bold = -1; Italic = -1 }
)

try {
  $hashes = @{}
  foreach ($family in $families) {
    foreach ($face in $faces) {
      $sourceFont = Join-Path $fonts (Join-Path $family.Dir ($family.Prefix + "-" + $face.Suffix + ".ttf"))
      Copy-Item -LiteralPath $sourceFont -Destination (Join-Path $renderFonts (Split-Path $sourceFont -Leaf)) -Force
    }
    $familyHashes = @{}
    foreach ($face in $faces) {
      $sourceFont = Join-Path $fonts (Join-Path $family.Dir ($family.Prefix + "-" + $face.Suffix + ".ttf"))
      $stem = ($family.Style + "-" + $face.Name)
      $ass = Join-Path $work ($stem + ".ass")
      $png = Join-Path $work ($stem + ".png")
      $content = @"
[Script Info]
ScriptType: v4.00+
PlayResX: 720
PlayResY: 400
ScaledBorderAndShadow: yes

[V4+ Styles]
Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding
Style: Matrix,$($family.Name),64,&H00FFFFFF,&H000000FF,&H00000000,&HFF000000,$($face.Bold),$($face.Italic),0,0,100,100,0,0,1,2,0,5,0,0,0,1

[Events]
Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text
Dialogue: 0,0:00:00.00,0:00:01.00,Matrix,,0,0,0,,{\an5\pos(360,200)}AspectShift Aa 123
"@
      Set-Content -LiteralPath $ass -Value $content -Encoding utf8
      $filterAss = $ass.Replace('\', '/').Replace(':', '\:').Replace("'", "\'")
      $filterFonts = $renderFonts.Replace('\', '/').Replace(':', '\:').Replace("'", "\'")
      $ErrorActionPreference = "Continue"
      $output = & $Ffmpeg -hide_banner -loglevel verbose -f lavfi -i "color=c=#202020:s=720x400:d=1" -vf "ass='$filterAss':fontsdir='$filterFonts'" -frames:v 1 -y $png 2>&1
      $ErrorActionPreference = "Stop"
      if ($LASTEXITCODE -ne 0) { throw ($output -join [Environment]::NewLine) }

      $expectedFace = [regex]::Escape((Split-Path $sourceFont -Leaf))
      if (-not (($output -join "`n") -match $expectedFace)) {
        throw "libass did not select $expectedFace for $($family.Name):`n$($output -join [Environment]::NewLine)"
      }
      $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $png).Hash
      if ($familyHashes.ContainsKey($hash)) {
        throw "$($family.Name) rendered $($face.Name) identically to $($familyHashes[$hash])"
      }
      $familyHashes[$hash] = $face.Name
    }
    $hashes[$family.Name] = $familyHashes.Count
  }

  $families | ForEach-Object {
    Write-Output "$($_.Style): $($_.Name) -> 4/4 bundled style faces"
  }
  Write-Output "PASS: all 40 text-overlay export font/style combinations resolved bundled font files and rendered distinctly."
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
