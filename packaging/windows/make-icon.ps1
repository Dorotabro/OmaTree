<#
.SYNOPSIS
  Regenerates windows/omatree.ico from the existing artwork in assets/logo.

.DESCRIPTION
  The .ico holds seven PNG-compressed images (Windows 10 and later read them
  all): 16, 24, 32, 48, 64, 128 and 256 pixels square. The 32, 64, 128 and 256
  pixel images are the existing assets/logo PNGs, byte for byte. Only 16, 24
  and 48 are derived, by scaling the 1024 pixel artwork down with high-quality
  bicubic resampling (the same way the macOS build derives its 16 pixel image).
  Nothing is redrawn.

  windows/omatree.ico is committed; run this only when the artwork changes.
  Output is deterministic for a given System.Drawing, but byte-identical
  output across Windows versions is not promised.
#>
[CmdletBinding()]
param(
    [string]$Repo = ''
)
$ErrorActionPreference = 'Stop'
if (-not $Repo) { $Repo = (Resolve-Path (Join-Path (Split-Path -Parent $MyInvocation.MyCommand.Path) '..\..')).Path }
Add-Type -AssemblyName System.Drawing

$logo = Join-Path $Repo 'assets\logo'
$out = Join-Path $Repo 'windows\omatree.ico'

function Get-PngBytes([int]$size) {
    $existing = Join-Path $logo "omatree-icon-$size.png"
    if (Test-Path $existing) { return [IO.File]::ReadAllBytes($existing) }
    # Derived size: scale the 1024 pixel artwork down.
    $src = [System.Drawing.Image]::FromFile((Join-Path $logo 'omatree-icon-1024.png'))
    try {
        $bmp = New-Object System.Drawing.Bitmap $size, $size, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
        $g = [System.Drawing.Graphics]::FromImage($bmp)
        $g.CompositingMode = 'SourceCopy'
        $g.InterpolationMode = 'HighQualityBicubic'
        $g.SmoothingMode = 'HighQuality'
        $g.PixelOffsetMode = 'HighQuality'
        $g.DrawImage($src, 0, 0, $size, $size)
        $g.Dispose()
        $ms = New-Object IO.MemoryStream
        $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
        $bmp.Dispose()
        return $ms.ToArray()
    } finally { $src.Dispose() }
}

$sizes = 16, 24, 32, 48, 64, 128, 256
$images = foreach ($s in $sizes) { , @($s, (Get-PngBytes $s)) }

$ms = New-Object IO.MemoryStream
$w = New-Object IO.BinaryWriter $ms
$w.Write([uint16]0); $w.Write([uint16]1); $w.Write([uint16]$sizes.Count)   # ICONDIR
$offset = 6 + 16 * $sizes.Count
foreach ($i in $images) {
    $s = $i[0]; $bytes = $i[1]
    $w.Write([byte]($(if ($s -ge 256) { 0 } else { $s })))   # width  (0 = 256)
    $w.Write([byte]($(if ($s -ge 256) { 0 } else { $s })))   # height
    $w.Write([byte]0); $w.Write([byte]0)                      # palette, reserved
    $w.Write([uint16]1); $w.Write([uint16]32)                 # planes, bits per pixel
    $w.Write([uint32]$bytes.Length); $w.Write([uint32]$offset)
    $offset += $bytes.Length
}
foreach ($i in $images) { $w.Write([byte[]]$i[1]) }
$w.Flush()
[IO.File]::WriteAllBytes($out, $ms.ToArray())
"{0}: {1} images ({2}) {3} bytes" -f $out, $sizes.Count, ($sizes -join ','), (Get-Item $out).Length
