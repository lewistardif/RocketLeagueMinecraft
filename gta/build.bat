@echo off
setlocal
set "GTA=%~dp0"
set "ROOT=%GTA%.."
pushd "%ROOT%" || exit /b 1

echo == cargo build -p rl_car_ffi --release
cargo build -p rl_car_ffi --release || goto :fail

echo == cmake (MSVC)
cmake -S "%GTA%." -B "%GTA%build" -G "Visual Studio 17 2022" -A x64 || goto :fail
cmake --build "%GTA%build" --config Release --parallel -- /nologo /v:minimal || goto :fail

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
