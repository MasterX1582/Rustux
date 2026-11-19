try {
    $r = Mount-DiskImage -ImagePath 'D:\Downloads\ZorinOS\Zorin OS 18.1 Pro.iso' -PassThru
    $letter = ($r | Get-Volume).DriveLetter
    Write-Output "MOUNTED:$letter"
    Get-ChildItem -Path "${letter}:\" | Select-Object -ExpandProperty Name
} catch {
    Write-Output ('FAIL: ' + $_.Exception.Message)
}
