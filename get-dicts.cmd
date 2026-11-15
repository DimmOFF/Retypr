@echo off
setlocal enableextensions
chcp 65001 >nul

rem Скачивает частотные словари (en/ru, по 50 тыс. слов) для автоопределения раскладки.
rem Источник: hermitdave/FrequencyWords (лицензия CC-BY-SA 4.0, данные OpenSubtitles).
rem Файлы сохраняются у пользователя, в сборку программы они не входят.

set DIR=%APPDATA%\retypr\config\dict
set BASE=https://raw.githubusercontent.com/hermitdave/FrequencyWords/master/content/2018
if not exist "%DIR%" mkdir "%DIR%"

echo [INFO] Папка: %DIR%
echo [INFO] Скачиваю en.txt и ru.txt ...
powershell -NoProfile -ExecutionPolicy Bypass -Command "$ErrorActionPreference='Stop'; [Net.ServicePointManager]::SecurityProtocol=[Net.SecurityProtocolType]::Tls12; Invoke-WebRequest -UseBasicParsing '%BASE%/en/en_50k.txt' -OutFile '%DIR%\en.txt'; Invoke-WebRequest -UseBasicParsing '%BASE%/ru/ru_50k.txt' -OutFile '%DIR%\ru.txt'"
if errorlevel 1 (
    echo.
    echo [ERROR] Не удалось скачать словари. Проверьте интернет и ссылки в этом скрипте.
    goto :end
)

echo [INFO] Скачиваю uk.txt (украинский, необязательно) ...
powershell -NoProfile -ExecutionPolicy Bypass -Command "$ErrorActionPreference='Stop'; [Net.ServicePointManager]::SecurityProtocol=[Net.SecurityProtocolType]::Tls12; Invoke-WebRequest -UseBasicParsing '%BASE%/uk/uk_50k.txt' -OutFile '%DIR%\uk.txt'"
if errorlevel 1 echo [WARN] uk.txt не скачан — украинский будет определяться только эвристикой.

if not exist "%DIR%\exceptions.txt" (
    > "%DIR%\exceptions.txt" echo # Words that must never be auto-converted, one per line (lowercase).
    >> "%DIR%\exceptions.txt" echo # Example: retypr
)

echo.
echo [OK] Готово. Перезапустите Retypr, чтобы он подхватил словари.

:end
echo.
pause
endlocal
