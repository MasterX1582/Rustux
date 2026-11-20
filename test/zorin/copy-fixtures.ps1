$dst = Join-Path $PSScriptRoot 'fixtures'
New-Item -ItemType Directory -Force -Path $dst | Out-Null
Copy-Item 'E:\casper\vmlinuz' "$dst\vmlinuz"
Copy-Item 'E:\casper\initrd.zstd' "$dst\initrd.zstd"
Copy-Item 'E:\boot\grub\grub.cfg' "$dst\grub.cfg"
Get-ChildItem $dst | Select-Object Name, @{N='SizeMB';E={[math]::Round($_.Length/1MB,1)}}
