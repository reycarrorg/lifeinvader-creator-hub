@echo off
cd /d "%~dp0"
if not exist "Lifeinvader.exe" (
  echo Lifeinvader.exe is missing from this folder. See README.md for build instructions.
  pause
  exit /b 1
)
start "" "%~dp0Lifeinvader.exe"
