@echo off
setlocal
set "SKY=%~dp0"
set "ROOT=%SKY%.."
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

if not exist "%SKY%extern\CommonLibSSE-NG\CMakeLists.txt" (
  echo == git submodule update CommonLibSSE-NG
  git submodule update --init --recursive skyrim/extern/CommonLibSSE-NG || goto :fail
)

set "VCPKG=%SKY%.tools\vcpkg"
if not exist "%VCPKG%\vcpkg.exe" (
  echo == vcpkg bootstrap
  if not exist "%VCPKG%\.git" git clone https://github.com/microsoft/vcpkg.git "%VCPKG%" || goto :fail
  call "%VCPKG%\bootstrap-vcpkg.bat" -disableMetrics || goto :fail
)

echo == cmake (MSVC, %CMAKE%)
"%CMAKE%" -S "%SKY%." -B "%SKY%build" -A x64 -DCMAKE_TOOLCHAIN_FILE="%VCPKG%\scripts\buildsystems\vcpkg.cmake" -DVCPKG_TARGET_TRIPLET=x64-windows-static-md || goto :fail
"%CMAKE%" --build "%SKY%build" --config Release --parallel -- /nologo /v:minimal || goto :fail

echo == unit tests
"%SKY%build\Release\tests.exe" "%ROOT%\target\release\rl_car_ffi.dll" || goto :fail

echo == staging (Mod Organizer layout)
if exist "%SKY%stage" rmdir /s /q "%SKY%stage"
mkdir "%SKY%stage\SKSE\Plugins\RLCar" || goto :fail
copy /y "%ROOT%\target\release\rl_car_ffi.dll" "%SKY%stage\SKSE\Plugins\RLCar\" >nul || goto :fail
copy /y "%SKY%RLCar.ini" "%SKY%stage\SKSE\Plugins\RLCar\" >nul || goto :fail
if not exist "%SKY%build\Release\RLCar.dll" (
  echo RLCar.dll was not built.
  goto :fail
)
copy /y "%SKY%build\Release\RLCar.dll" "%SKY%stage\SKSE\Plugins\" >nul || goto :fail
echo BUILD OK: %SKY%stage
popd
exit /b 0

:fail
popd
echo BUILD FAILED
exit /b 1
