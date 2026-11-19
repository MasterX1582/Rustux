Get-ChildItem -Path 'E:\casper' | Select-Object Name, @{N='SizeMB';E={[math]::Round($_.Length/1MB,1)}} | Format-Table -AutoSize
Write-Output '--- grub.cfg kernel cmdline ---'
Get-Content 'E:\boot\grub\grub.cfg' | Select-String 'linux|initrd' | Select-Object -First 6
