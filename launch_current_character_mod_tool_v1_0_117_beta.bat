@echo off
setlocal
cd /d "%~dp0"
if exist "release\CharacterModTool-v1.0.117-beta-PrivateBeta\CharacterModTool.exe" (
    start "" "release\CharacterModTool-v1.0.117-beta-PrivateBeta\CharacterModTool.exe"
    exit /b 0
)
call "Launch Character Mod Tool.bat"
