@echo off
setlocal
cd /d "%~dp0"

if exist "CharacterModTool.exe" (
    start "" "CharacterModTool.exe"
    exit /b 0
)

if exist "release\CharacterModTool-v1.0.118-beta-PrivateBeta\CharacterModTool.exe" (
    start "" "release\CharacterModTool-v1.0.118-beta-PrivateBeta\CharacterModTool.exe"
    exit /b 0
)

if exist ".release_venv\Scripts\pythonw.exe" (
    start "" ".release_venv\Scripts\pythonw.exe" "character_mod_tool.py"
    exit /b 0
)

where py >nul 2>nul
if not errorlevel 1 (
    py -3 character_mod_tool.py
    exit /b %errorlevel%
)

where python >nul 2>nul
if not errorlevel 1 (
    python character_mod_tool.py
    exit /b %errorlevel%
)

echo Character Mod Tool could not find CharacterModTool.exe or Python 3.
echo Use the portable release build, or install Python 3 with Tk support.
pause
exit /b 1
