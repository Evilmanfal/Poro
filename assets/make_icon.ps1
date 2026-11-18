# Builds assets/poro.ico and assets/poro.png from a transparent PNG.
# Trims the transparent border, centers the image on a square with a small
# margin, and writes a multi-size .ico (16-256 px, PNG-compressed entries).
# Usage: ./assets/make_icon.ps1 <source.png>
param([Parameter(Mandatory = $true)][string]$Source)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type -ReferencedAssemblies System.Drawing -TypeDefinition @'
using System; using System.Drawing; using System.Drawing.Imaging; using System.Runtime.InteropServices;
public static class Opaque {
    // Bounding box of pixels with alpha above 8.
    public static Rectangle Bounds(Bitmap b) {
        var d = b.LockBits(new Rectangle(0, 0, b.Width, b.Height), ImageLockMode.ReadOnly, PixelFormat.Format32bppArgb);
        var px = new byte[d.Stride * b.Height]; Marshal.Copy(d.Scan0, px, 0, px.Length); b.UnlockBits(d);
        int minX = b.Width, minY = b.Height, maxX = -1, maxY = -1;
        for (int y = 0; y < b.Height; y++) for (int x = 0; x < b.Width; x++) {
            if (px[y * d.Stride + x * 4 + 3] > 8) { if (x < minX) minX = x; if (x > maxX) maxX = x; if (y < minY) minY = y; if (y > maxY) maxY = y; }
        }
        return maxX < 0 ? new Rectangle(0, 0, b.Width, b.Height) : Rectangle.FromLTRB(minX, minY, maxX + 1, maxY + 1);
    }
}
'@
$src = [System.Drawing.Bitmap]::FromFile((Resolve-Path $Source))
$box = [Opaque]::Bounds($src)
$side = [int]([Math]::Max($box.Width, $box.Height) * 1.04)

function New-Square([int]$size) {
    $bmp = New-Object System.Drawing.Bitmap $size, $size, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.Clear([System.Drawing.Color]::Transparent)
    $g.InterpolationMode = 'HighQualityBicubic'; $g.PixelOffsetMode = 'HighQuality'; $g.SmoothingMode = 'HighQuality'; $g.CompositingQuality = 'HighQuality'
    $scale = $size / $side
    $w = $box.Width * $scale; $h = $box.Height * $scale
    $dest = New-Object System.Drawing.RectangleF (($size - $w) / 2), (($size - $h) / 2), $w, $h
    $g.DrawImage($src, $dest, (New-Object System.Drawing.RectangleF $box.X, $box.Y, $box.Width, $box.Height), [System.Drawing.GraphicsUnit]::Pixel)
    $g.Dispose()
    return $bmp
}

$dir = Split-Path -Parent $MyInvocation.MyCommand.Path
$sizes = 16, 20, 24, 32, 40, 48, 64, 128, 256
$pngs = foreach ($s in $sizes) {
    $bmp = New-Square $s
    $ms = New-Object System.IO.MemoryStream
    $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png); $bmp.Dispose()
    , $ms.ToArray()
}
# In-app logo.
$logo = New-Square 128; $logo.Save((Join-Path $dir 'poro.png'), [System.Drawing.Imaging.ImageFormat]::Png); $logo.Dispose()
$src.Dispose()

# ICO: header, one 16-byte entry per size, then the PNG data.
$out = New-Object System.IO.MemoryStream
$w = New-Object System.IO.BinaryWriter $out
$w.Write([UInt16]0); $w.Write([UInt16]1); $w.Write([UInt16]$sizes.Count)
$offset = 6 + 16 * $sizes.Count
for ($i = 0; $i -lt $sizes.Count; $i++) {
    $s = $sizes[$i]; $len = $pngs[$i].Length
    $dim = if ($s -ge 256) { 0 } else { $s }
    $w.Write([byte]$dim); $w.Write([byte]$dim); $w.Write([byte]0); $w.Write([byte]0)
    $w.Write([UInt16]1); $w.Write([UInt16]32); $w.Write([UInt32]$len); $w.Write([UInt32]$offset)
    $offset += $len
}
foreach ($p in $pngs) { $w.Write($p) }
$w.Flush()
[System.IO.File]::WriteAllBytes((Join-Path $dir 'poro.ico'), $out.ToArray())
"wrote poro.ico ($($sizes -join ', ') px) and poro.png; cropped to $($box.Width)x$($box.Height)"
