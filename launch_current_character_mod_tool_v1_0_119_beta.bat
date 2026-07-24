@echo off
setlocal
cd /d "%~dp0"
if exist "release\CharacterModTool-v1.0.119-beta-PrivateBeta\CharacterModTool.exe" (
    start "" "release\CharacterModTool-v1.0.119-beta-PrivateBeta\CharacterModTool.exe"
    exit /b 0
)
echo Character Mod Tool v1.0.119-beta was not found.
echo Build or extract the current Private Beta release first.
pause
