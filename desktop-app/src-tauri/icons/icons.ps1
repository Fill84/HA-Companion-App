# Forceer de locatie naar de map waar dit script staat
$PSScriptRoot = Split-Path -Parent -Path $MyInvocation.MyCommand.Definition
Set-Location $PSScriptRoot

# Naam van je bronbestand
$sourceFile = "512x512.png"
$fullPath = Join-Path $PSScriptRoot $sourceFile

# Check of het bestand er echt is
if (-not (Test-Path $fullPath)) {
    Write-Host "FOUT: Kan $sourceFile niet vinden in $PSScriptRoot" -ForegroundColor Red
    return
}

$sizes = @(
    @{Size = 32; Name = "32x32.png" },
    @{Size = 128; Name = "128x128.png" },
    @{Size = 256; Name = "128x128@2.png" },
    @{Size = 256; Name = "icon.png" }
)

Add-Type -AssemblyName System.Drawing

try {
    $srcImg = [System.Drawing.Image]::FromFile($fullPath)

    foreach ($s in $sizes) {
        $newImg = New-Object System.Drawing.Bitmap($s.Size, $s.Size)
        $g = [System.Drawing.Graphics]::FromImage($newImg)
        
        # Behoud transparantie en zet kwaliteit hoog
        $g.Clear([System.Drawing.Color]::Transparent)
        $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
        $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
        
        $g.DrawImage($srcImg, 0, 0, $s.Size, $s.Size)
        
        $outputPath = Join-Path $PSScriptRoot $s.Name
        $newImg.Save($outputPath, [System.Drawing.Imaging.ImageFormat]::Png)
        
        $g.Dispose()
        $newImg.Dispose()
        Write-Host "Succes: $($s.Name) is opgeslagen." -ForegroundColor Green
    }
}
finally {
    if ($null -ne $srcImg) { $srcImg.Dispose() }
}

# --- Deel 2: Maak een icon.ico (256x256 basis) ---
$icoPath = Join-Path $PSScriptRoot "icon.ico"
$largeIconPath = Join-Path $PSScriptRoot "icon.png" # Gebruik de 256x256 die we net maakten

if (Test-Path $largeIconPath) {
    $bitmap = [System.Drawing.Bitmap]::FromFile($largeIconPath)
    $hIcon = $bitmap.GetHicon()
    $icon = [System.Drawing.Icon]::FromHandle($hIcon)
    
    # Maak een FileStream om het icoon op te slaan
    $fileStream = New-Object System.IO.FileStream($icoPath, [System.IO.FileMode]::Create)
    $icon.Save($fileStream)
    
    # Ruim het geheugen netjes op
    $fileStream.Close()
    $icon.Dispose()
    $signature = '[DllImport("user32.dll")] public static extern bool DestroyIcon(IntPtr hIcon);'
    $type = Add-Type -MemberDefinition $signature -Name "NativeMethods" -Namespace "Win32" -PassThru
    $type::DestroyIcon($hIcon) | Out-Null
    $bitmap.Dispose()

    Write-Host "Succes: icon.ico is aangemaakt voor Windows." -ForegroundColor Cyan
}
else {
    Write-Host "FOUT: Kan $largeIconPath niet vinden om icon.ico te maken." -ForegroundColor Red
}