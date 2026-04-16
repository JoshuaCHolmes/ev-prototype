<# 
.SYNOPSIS
    Flash ESP32 firmware for EV Prototype
.DESCRIPTION
    Downloads and flashes the latest ESP32 firmware from GitHub releases.
    Requires Python with esptool installed, or will attempt to install it.
#>

param(
    [string]$ComPort = "",
    [switch]$ListPorts
)

$ErrorActionPreference = "Stop"
$RepoOwner = "JoshuaCHolmes"
$RepoName = "ev-prototype"

Write-Host "═══════════════════════════════════════════════════════════" -ForegroundColor Cyan
Write-Host "       ESP32 Firmware Flasher - EV Prototype" -ForegroundColor Cyan
Write-Host "       Texas A&M FLiNT - Team Autopilot" -ForegroundColor Cyan
Write-Host "═══════════════════════════════════════════════════════════" -ForegroundColor Cyan
Write-Host ""

# Find COM ports
function Get-ESP32Ports {
    $ports = Get-WmiObject Win32_SerialPort | Where-Object { 
        $_.Description -match "CP210|CH340|USB.*Serial" 
    }
    return $ports
}

if ($ListPorts) {
    Write-Host "Available serial ports:" -ForegroundColor Yellow
    $ports = Get-ESP32Ports
    if ($ports) {
        foreach ($p in $ports) {
            Write-Host "  $($p.DeviceID) - $($p.Description)" -ForegroundColor Green
        }
    } else {
        Write-Host "  (none found)" -ForegroundColor Red
    }
    exit 0
}

# Auto-detect COM port if not specified
if (-not $ComPort) {
    Write-Host "[*] Auto-detecting ESP32..." -ForegroundColor Yellow
    $ports = Get-ESP32Ports
    if ($ports) {
        $ComPort = ($ports | Select-Object -First 1).DeviceID
        Write-Host "[+] Found ESP32 on $ComPort" -ForegroundColor Green
    } else {
        Write-Host "[!] No ESP32 found. Please specify -ComPort COMx" -ForegroundColor Red
        Write-Host ""
        Write-Host "Available ports:"
        Get-WmiObject Win32_SerialPort | ForEach-Object { 
            Write-Host "  $($_.DeviceID) - $($_.Description)" 
        }
        exit 1
    }
}

# Check for Python
Write-Host "[*] Checking Python..." -ForegroundColor Yellow
$python = $null
foreach ($cmd in @("python", "python3", "py")) {
    try {
        $ver = & $cmd --version 2>&1
        if ($ver -match "Python 3") {
            $python = $cmd
            Write-Host "[+] Found $ver" -ForegroundColor Green
            break
        }
    } catch {}
}

if (-not $python) {
    Write-Host "[!] Python 3 not found. Please install Python from python.org" -ForegroundColor Red
    exit 1
}

# Check/install esptool
Write-Host "[*] Checking esptool..." -ForegroundColor Yellow
$esptoolCheck = & $python -c "import esptool" 2>&1
if ($LASTEXITCODE -ne 0) {
    Write-Host "[*] Installing esptool..." -ForegroundColor Yellow
    & $python -m pip install esptool --quiet
}
Write-Host "[+] esptool ready" -ForegroundColor Green

# Get latest release from GitHub
Write-Host "[*] Fetching latest release..." -ForegroundColor Yellow
$releaseUrl = "https://api.github.com/repos/$RepoOwner/$RepoName/releases/latest"
try {
    $release = Invoke-RestMethod -Uri $releaseUrl -Headers @{"User-Agent"="PowerShell"}
    $version = $release.tag_name
    Write-Host "[+] Latest version: $version" -ForegroundColor Green
} catch {
    Write-Host "[!] Could not fetch release info: $_" -ForegroundColor Red
    exit 1
}

# Find firmware binary in release assets
$firmwareAsset = $release.assets | Where-Object { $_.name -match "firmware.*\.bin$" }
if (-not $firmwareAsset) {
    Write-Host "[!] No firmware.bin found in release assets" -ForegroundColor Red
    Write-Host "    Available assets:" -ForegroundColor Yellow
    $release.assets | ForEach-Object { Write-Host "      - $($_.name)" }
    Write-Host ""
    Write-Host "    Please compile firmware manually using Arduino IDE" -ForegroundColor Yellow
    exit 1
}

# Download firmware
$tempDir = Join-Path $env:TEMP "ev-prototype-firmware"
New-Item -ItemType Directory -Force -Path $tempDir | Out-Null
$firmwarePath = Join-Path $tempDir $firmwareAsset.name

Write-Host "[*] Downloading $($firmwareAsset.name)..." -ForegroundColor Yellow
Invoke-WebRequest -Uri $firmwareAsset.browser_download_url -OutFile $firmwarePath
Write-Host "[+] Downloaded to $firmwarePath" -ForegroundColor Green

# Flash firmware
Write-Host ""
Write-Host "═══════════════════════════════════════════════════════════" -ForegroundColor Cyan
Write-Host "       FLASHING FIRMWARE - DO NOT DISCONNECT!" -ForegroundColor Red
Write-Host "═══════════════════════════════════════════════════════════" -ForegroundColor Cyan
Write-Host ""

& $python -m esptool --chip esp32 --port $ComPort --baud 921600 write_flash 0x10000 $firmwarePath

if ($LASTEXITCODE -eq 0) {
    Write-Host ""
    Write-Host "═══════════════════════════════════════════════════════════" -ForegroundColor Green
    Write-Host "       FIRMWARE UPDATE COMPLETE!" -ForegroundColor Green
    Write-Host "       Version: $version" -ForegroundColor Green
    Write-Host "═══════════════════════════════════════════════════════════" -ForegroundColor Green
} else {
    Write-Host ""
    Write-Host "[!] Flash failed. Try:" -ForegroundColor Red
    Write-Host "    1. Hold BOOT button on ESP32 while flashing" -ForegroundColor Yellow
    Write-Host "    2. Try a different USB cable" -ForegroundColor Yellow
    Write-Host "    3. Check COM port is correct" -ForegroundColor Yellow
}

Write-Host ""
Write-Host "Press any key to exit..."
$null = $Host.UI.RawUI.ReadKey("NoEcho,IncludeKeyDown")
