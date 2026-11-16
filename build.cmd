@echo off
setlocal enableextensions
chcp 65001 >nul
cd /d "%~dp0"

rem Usage:  build.cmd [debug] [run] [rebuild]
rem   debug   - debug build (console window + logs), default is release
rem   run     - start the built exe after a successful build
rem   rebuild - force a clean rebuild of the retypr crate (dependencies stay cached)
rem Requires: Rust (https://rustup.rs) and MSVC Build Tools ("Desktop development with C++")

set "RC=0"
set "PROFILE=release"
set "CARGO_FLAGS=--release"
set "RUN_AFTER=0"
set "REBUILD=0"
for %%A in (%*) do (
    if /i "%%A"=="debug" (
        set "PROFILE=debug"
        set "CARGO_FLAGS="
    )
    if /i "%%A"=="run" set "RUN_AFTER=1"
    if /i "%%A"=="rebuild" set "REBUILD=1"
)

set "SRC=target\x86_64-pc-windows-msvc\%PROFILE%\Retypr.exe"
set "OUT=dist\windows-x64"

where cargo >nul 2>nul
if errorlevel 1 (
    echo [ERROR] cargo not found. Install Rust: https://rustup.rs
    set RC=1
    goto :end
)

rem --- Make sure cargo notices changed sources (zip extraction can leave old timestamps) ---
if exist "%~dp0tools\sync-mtime.ps1" (
    powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0tools\sync-mtime.ps1"
)

if "%REBUILD%"=="1" (
    echo [INFO] Cleaning the retypr crate...
    cargo clean -p retypr
)

set "BEFORE="
if exist "%SRC%" for %%F in ("%SRC%") do set "BEFORE=%%~tF %%~zF"

echo [INFO] Building retypr (%PROFILE%) for Windows...
cargo build %CARGO_FLAGS% --target x86_64-pc-windows-msvc
if errorlevel 1 (
    echo.
    echo [ERROR] Build failed.
    set RC=1
    goto :end
)

if not exist "%SRC%" (
    echo [ERROR] %SRC% was not produced.
    set RC=1
    goto :end
)
set "AFTER="
for %%F in ("%SRC%") do set "AFTER=%%~tF %%~zF"
if "%BEFORE%"=="%AFTER%" (
    echo [NOTE] The exe is unchanged - cargo found nothing to rebuild. If you expected changes, run: build.cmd rebuild
)

if not exist "%OUT%" mkdir "%OUT%"

rem --- copy with retries (antivirus / explorer may hold the file for a moment) ---
set TRIES=0
:copy_retry
copy /y "%SRC%" "%OUT%\Retypr.exe" >nul 2>nul
if not errorlevel 1 goto :copied
set /a TRIES+=1
if %TRIES% lss 5 (
    timeout /t 1 /nobreak >nul
    goto :copy_retry
)
echo [ERROR] Could not copy Retypr.exe to %OUT% - the file is locked by another process.
set RC=1
goto :end

:copied
copy /y README.md "%OUT%\" >nul 2>nul
if exist "assets\sounds" xcopy /e /i /y /q "assets\sounds" "%OUT%\data\sounds\" >nul 2>nul

echo.
echo [OK] Built:  %SRC%
for %%F in ("%SRC%") do echo      source: %%~tF   %%~zF bytes
for %%F in ("%OUT%\Retypr.exe") do echo      dist:   %%~tF   %%~zF bytes

if "%RUN_AFTER%"=="1" (
    echo [INFO] Starting %OUT%\Retypr.exe ...
    start "" "%OUT%\Retypr.exe"
)

:end
echo.
pause
endlocal & exit /b %RC%
