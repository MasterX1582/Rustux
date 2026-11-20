# gen_logo.ps1 — project logo pipeline.
#
# 1. Converts the background of the source image to alpha=0 by flood-filling
#    from the edges; the background color range is learned from the border
#    pixels (works for black, white, or any flat backdrop; interior pixels
#    of the same color stay opaque because the fill is 4-connected).
#    Result: docs/logo.png (used by the README).
# 2. Composites onto black, downscales, and emits kernel/src/logo_data.rs —
#    a packed 24-bit RGB array the kernel blits to the GOP framebuffer as the
#    default boot logo.
#
# Usage: pwsh gen_logo.ps1 -Src <path-to-source.png> [-OutW 96]
param(
    [Parameter(Mandatory = $true)][string]$Src,
    [int]$OutW = 96
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

$repo = $PSScriptRoot
$srcBmp = New-Object System.Drawing.Bitmap($Src)
$w = $srcBmp.Width
$h = $srcBmp.Height

# --- 1. Edge flood-fill transparency -----------------------------------
$rect = New-Object System.Drawing.Rectangle(0, 0, $w, $h)
$bd = $srcBmp.LockBits($rect, [System.Drawing.Imaging.ImageLockMode]::ReadOnly,
    [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
$stride = $bd.Stride
$buf = New-Object byte[] ($stride * $h)
[System.Runtime.InteropServices.Marshal]::Copy($bd.Scan0, $buf, 0, $buf.Length)
$srcBmp.UnlockBits($bd)

Write-Host 'locked bits'
$visited = New-Object bool[] ($w * $h)
$queue = New-Object 'System.Collections.Generic.Queue[int]'
Write-Host 'arrays ok'

# Learn the background color from the border pixels instead of assuming
# near-black: min/max per channel across all edges plus a margin covers
# flat backdrops with JPEG noise or a gentle gradient. Interior pixels of
# the same color stay opaque because the fill only walks 4-connected
# background from the edges.
[int]$rMin = 255; [int]$rMax = 0; [int]$gMin = 255; [int]$gMax = 0
[int]$bMin = 255; [int]$bMax = 0
function Scan-Pix($x, $y) {
    $o = $y * $stride + $x * 4
    if ($buf[$o] -lt $script:bMin) { $script:bMin = [int]$buf[$o] }
    if ($buf[$o] -gt $script:bMax) { $script:bMax = [int]$buf[$o] }
    if ($buf[$o + 1] -lt $script:gMin) { $script:gMin = [int]$buf[$o + 1] }
    if ($buf[$o + 1] -gt $script:gMax) { $script:gMax = [int]$buf[$o + 1] }
    if ($buf[$o + 2] -lt $script:rMin) { $script:rMin = [int]$buf[$o + 2] }
    if ($buf[$o + 2] -gt $script:rMax) { $script:rMax = [int]$buf[$o + 2] }
}
for ($x = 0; $x -lt $w; $x += 8) { Scan-Pix $x 0; Scan-Pix $x ($h - 1) }
for ($y = 0; $y -lt $h; $y += 8) { Scan-Pix 0 $y; Scan-Pix ($w - 1) $y }
$m = 16
$bMin = [Math]::Max(0, ($bMin - $m)); $bMax = [Math]::Min(255, ($bMax + $m))
$gMin = [Math]::Max(0, ($gMin - $m)); $gMax = [Math]::Min(255, ($gMax + $m))
$rMin = [Math]::Max(0, ($rMin - $m)); $rMax = [Math]::Min(255, ($rMax + $m))
Write-Output "bg range r[$rMin,$rMax] g[$gMin,$gMax] b[$bMin,$bMax]"

function Push-IfBg($x, $y) {
    $i = $y * $w + $x
    if ($visited[$i]) { return }
    $o = $y * $stride + $x * 4
    if ($buf[$o] -ge $bMin -and $buf[$o] -le $bMax -and
        $buf[$o + 1] -ge $gMin -and $buf[$o + 1] -le $gMax -and
        $buf[$o + 2] -ge $rMin -and $buf[$o + 2] -le $rMax) {
        $visited[$i] = $true
        $script:queue.Enqueue($i)
    }
}

for ($x = 0; $x -lt $w; $x++) { Push-IfBg $x 0; Push-IfBg $x ($h - 1) }
for ($y = 0; $y -lt $h; $y++) { Push-IfBg 0 $y; Push-IfBg ($w - 1) $y }
Write-Host 'seeded'

while ($queue.Count -gt 0) {
    $i = $queue.Dequeue()
    $x = $i % $w; $y = [int][Math]::Floor($i / $w)
    if ($x -gt 0)       { Push-IfBg ($x - 1) $y }
    if ($x -lt $w - 1)  { Push-IfBg ($x + 1) $y }
    if ($y -gt 0)       { Push-IfBg $x ($y - 1) }
    if ($y -lt $h - 1)  { Push-IfBg $x ($y + 1) }
}

# Write back: visited pixels become fully transparent; others keep color.
Write-Host 'fill done'
$alpha = New-Object byte[] ($stride * $h)
for ($y = 0; $y -lt $h; $y++) {
    for ($x = 0; $x -lt $w; $x++) {
        $i = $y * $w + $x
        $o = $y * $stride + $x * 4
        if ($visited[$i]) {
            $alpha[$o + 3] = 0
        } else {
            $alpha[$o] = $buf[$o]; $alpha[$o + 1] = $buf[$o + 1]
            $alpha[$o + 2] = $buf[$o + 2]; $alpha[$o + 3] = 255
        }
    }
}
$out = New-Object System.Drawing.Bitmap($w, $h, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
$od = $out.LockBits($rect, [System.Drawing.Imaging.ImageLockMode]::WriteOnly,
    [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
[System.Runtime.InteropServices.Marshal]::Copy($alpha, 0, $od.Scan0, $alpha.Length)
$out.UnlockBits($od)
$png = Join-Path $repo 'docs/logo.png'
$out.Save($png, [System.Drawing.Imaging.ImageFormat]::Png)
$nvis = ($visited | Where-Object { $_ }).Count
Write-Output "wrote $png ($w x $h, $nvis pixels made transparent)"

# --- 2. Downscale onto black, emit Rust RGB data --------------------------
$nh = [int]($h * $OutW / $w)
$small = New-Object System.Drawing.Bitmap($OutW, $nh)
$g = [System.Drawing.Graphics]::FromImage($small)
$g.Clear([System.Drawing.Color]::Black)
$g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
$g.DrawImage($out, 0, 0, $OutW, $nh)
$g.Dispose()

$rgb = New-Object byte[] ($OutW * $nh * 3)
for ($y = 0; $y -lt $nh; $y++) {
    for ($x = 0; $x -lt $OutW; $x++) {
        $c = $small.GetPixel($x, $y)
        $o = ($y * $OutW + $x) * 3
        $rgb[$o] = $c.R; $rgb[$o + 1] = $c.G; $rgb[$o + 2] = $c.B
    }
}
$small.Dispose(); $out.Dispose(); $srcBmp.Dispose()

$rs = Join-Path $repo 'kernel/src/logo_data.rs'
$sw = New-Object System.IO.StreamWriter($rs)
$sw.WriteLine("//! logo_data - generated by gen_logo.ps1 from docs/logo.png; do not edit.")
$sw.WriteLine("//! 24-bit RGB, row-major, already composited onto black.")
$sw.WriteLine("pub const LOGO_W: usize = $OutW;")
$sw.WriteLine("pub const LOGO_H: usize = $nh;")
$sw.WriteLine("pub static LOGO_RGB: [u8; LOGO_W * LOGO_H * 3] = [")
for ($i = 0; $i -lt $rgb.Length; $i += 24) {
    $line = ($rgb[$i..([Math]::Min($i + 23, $rgb.Length - 1))] | ForEach-Object { $_.ToString() }) -join ','
    $sw.WriteLine("    $line,")
}
$sw.WriteLine("];")
$sw.Close()
Write-Output "wrote $rs (${OutW}x${nh}, $($rgb.Length) bytes)"
