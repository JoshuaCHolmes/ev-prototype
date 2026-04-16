@echo off
setlocal enabledelayedexpansion
title Attach EV USB Devices to WSL
echo ============================================================
echo   Attaching EV Prototype USB devices to WSL
echo ============================================================
echo.

echo Looking for ESP32 (CP2102)...
for /f "tokens=1" %%i in ('usbipd list ^| findstr /i "CP2102"') do (
    echo Found at %%i - attaching...
    usbipd bind --busid %%i 2>nul
    usbipd attach --wsl --busid %%i
    echo [OK] ESP32 processed
)

echo.
echo Looking for cameras (Innomaker)...
for /f "tokens=1" %%i in ('usbipd list ^| findstr /i "Innomaker"') do (
    echo Found camera at %%i - attaching...
    usbipd bind --busid %%i 2>nul
    usbipd attach --wsl --busid %%i
    echo [OK] Camera processed
)

echo.
echo ============================================================
echo Done! Now run the controller in WSL:
echo   cd ~/personal/ev-prototype/wsl ^&^& ./run.sh tui
echo ============================================================
pause
