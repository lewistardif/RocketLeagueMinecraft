@echo off
setlocal
set "GTA=%~dp0"
set "ROOT=%GTA%.."
pushd "%ROOT%" || exit /b 1

echo == cargo build -p rl_car_ffi --release
cargo build -p rl_car_ffi --release || goto :fail

rem The newest Visual Studio with the C++ tools, and the CMake it ships (it knows that VS's generator).
set "CMAKE=cmake"
set "VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
set "VSDIR="
if exist "%VSWHERE%" for /f "usebackq delims=" %%i in (`"%VSWHERE%" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`) do set "VSDIR=%%i"
set "VSCMAKE=%VSDIR%\Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin\cmake.exe"
if defined VSDIR if exist "%VSCMAKE%" set "CMAKE=%VSCMAKE%"

echo == cmake (MSVC, %CMAKE%)
"%CMAKE%" -S "%GTA%." -B "%GTA%build" -A x64 || goto :fail
"%CMAKE%" --build "%GTA%build" --config Release --parallel -- /nologo /v:minimal || goto :fail

echo == unit tests
"%GTA%build\Release\rlcar_tests.exe" "%ROOT%\target\release\rl_car_ffi.dll" || goto :fail

echo == staging
if exist "%GTA%stage" rmdir /s /q "%GTA%stage"
mkdir "%GTA%stage\RLCar" || goto :fail
copy /y "%ROOT%\target\release\rl_car_ffi.dll" "%GTA%stage\RLCar\" >nul || goto :fail
copy /y "%GTA%RLCar.ini" "%GTA%stage\RLCar\" >nul || goto :fail
if exist "%GTA%build\Release\RLCar.asi" (
  copy /y "%GTA%build\Release\RLCar.asi" "%GTA%stage\" >nul || goto :fail
) else (
  echo Script Hook V SDK missing: RLCar.asi was not built ^(set SHV_SDK^).
)
echo BUILD OK: %GTA%stage
popd
exit /b 0

:fail
popd
echo BUILD FAILED
exit /b 1
