@echo off
chcp 65001 >nul
rem ===================================================================
rem  Сглаживание изображений (проект АКСФ) - запуск в один клик
rem  Что делает: собирает программу, обрабатывает ВСЕ картинки из
rem  папки my_images всеми методами и складывает результаты в my_results
rem ===================================================================
cd /d "%~dp0"
echo ============================================================
echo   Сглаживание изображений: адаптивный контрастно-структурный
echo   фильтр (АКСФ) - учебный проект
echo ============================================================
echo.

where cargo >nul 2>nul
if errorlevel 1 (
    echo [!] Не найден Rust ^(команда cargo^).
    echo     Установите его один раз: скачайте rustup-init.exe
    echo     с сайта https://rustup.rs , запустите и нажмите Enter.
    echo     После установки запустите этот файл снова.
    echo.
    pause
    exit /b 1
)

echo [1/3] Сборка программы (первый раз может занять 1-2 минуты)...
cargo build --release
if errorlevel 1 (
    echo [!] Сборка не удалась. Скопируйте текст ошибки и пришлите в чат.
    pause
    exit /b 1
)

if not exist my_images mkdir my_images
if not exist my_results mkdir my_results

echo.
echo [2/3] Обрабатываю картинки из папки my_images ...
echo       (положите туда свои фото в формате jpg/png и запустите снова)
echo.
target\release\smoothing_project.exe demo -d my_images -o my_results --rgb --sigma 25

echo.
echo [3/3] Готово! Открываю папку с результатами...
start "" my_results
echo.
echo Подсказка: файл с окончанием _1_acsf.png - это результат НАШЕГО метода.
echo Чтобы изменить уровень шума, откройте этот файл Блокнотом и замените
echo --sigma 25 на, например, --sigma 10 или --sigma 40.
echo.
pause
