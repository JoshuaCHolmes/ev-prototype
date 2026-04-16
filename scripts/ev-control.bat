@echo off
title EV Prototype Control Center
"%~dp0ev-control.exe" %*
if errorlevel 1 (
    echo.
    echo [Error occurred]
)
pause
