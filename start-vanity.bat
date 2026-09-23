@echo off
setlocal
title Vanity Generator
for /f "tokens=2 delims=:" %%C in ('chcp') do set "vanity_codepage=%%C"
chcp 65001 >nul
cd /d "%~dp0"
if exist "vanitybtc.exe" (
  set "vanity_binary=vanitybtc.exe"
) else if exist "target\release\vanitybtc.exe" (
  set "vanity_binary=target\release\vanitybtc.exe"
) else (
  echo Build the app once with: cargo build --release --locked --offline
  pause
  if defined vanity_codepage chcp %vanity_codepage% >nul
  exit /b 1
)
"%vanity_binary%" --interactive
set "vanity_status=%errorlevel%"
pause
if defined vanity_codepage chcp %vanity_codepage% >nul
exit /b %vanity_status%
