# Builds Matchup on Windows.
# Usage: ./build.ps1 [build|run|test|check] [-Release] [-- extra cargo args]
#
# With a normal setup (rustup, Visual Studio Build Tools with the C++ workload,
# CMake on PATH), run this from a "Developer PowerShell for VS" or just use
# cargo directly. If MATCHUP_TOOLROOT points at a self-contained toolchain folder
# (rustup/, cargo-online/, tools/cmake-*/), that is used instead.
param([string]$Mode='build',[switch]$Release,[Parameter(ValueFromRemainingArguments=$true)]$Rest)
$env:CARGO_TARGET_DIR="$PSScriptRoot/target"
$toolRoot=$env:MATCHUP_TOOLROOT
if(-not $toolRoot){
    # The developer's own layout: a toolchain folder next to this repo.
    $guess=Join-Path $PSScriptRoot '../Codex/2026-10-08/can-x20/work'
    if(Test-Path $guess){$toolRoot=(Resolve-Path $guess).Path}
}
if($toolRoot){
    $env:RUSTUP_HOME="$toolRoot/rustup"
    $env:CARGO_HOME="$toolRoot/cargo-online"
    $compilerRoot='C:/Program Files/Microsoft Visual Studio/2022/Community/VC/Tools/MSVC/14.38.33130'
    $sdkRoot='C:/Program Files (x86)/Windows Kits/10'
    $sdkVersion='10.0.22000.0'
    $env:INCLUDE="$compilerRoot/include;$sdkRoot/Include/$sdkVersion/ucrt;$sdkRoot/Include/$sdkVersion/shared;$sdkRoot/Include/$sdkVersion/um;$sdkRoot/Include/$sdkVersion/winrt"
    $env:LIB="$compilerRoot/lib/x64;$sdkRoot/Lib/$sdkVersion/ucrt/x64;$sdkRoot/Lib/$sdkVersion/um/x64"
    $env:PATH="$toolRoot/rustup/toolchains/stable-x86_64-pc-windows-msvc/bin;$compilerRoot/bin/Hostx64/x64;$sdkRoot/bin/$sdkVersion/x64;$toolRoot/tools/cmake-3.31.8-windows-x86_64/bin;$toolRoot/cargo/bin;$env:PATH"
    $env:CMAKE_GENERATOR='NMake Makefiles'
}
Push-Location $PSScriptRoot
try {
    $arguments=@($Mode)
    if($Release){$arguments+='--release'}
    if($Rest){$arguments+=$Rest}
    & cargo @arguments 2>&1 | ForEach-Object { "$_" } | Where-Object { $_ -notmatch 'canonicalize' }
    exit $LASTEXITCODE
}finally{Pop-Location}
