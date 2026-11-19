# Builds and signs a Poro release, ready to upload to GitHub.
# Usage: ./release.ps1
#
# 1. Bump `version` in Cargo.toml (e.g. 0.1.0 -> 0.1.1) and commit.
# 2. Run this. It builds target/release/poro.exe, signs it with your release
#    key (%APPDATA%\Poro-release\release.key) and puts poro.exe and
#    poro.exe.sig in dist/.
# 3. Create the GitHub release with the printed command (or by hand). The tag
#    must be v<version>, and both files must be attached with exactly these
#    names. Installed copies offer the update within 12 hours or at their
#    next start.
# Exit codes are checked by hand: cargo writes progress to stderr, which
# Windows PowerShell would otherwise treat as a failure.
Push-Location $PSScriptRoot
try {
    $version = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
    & ./build.ps1 build -Release
    if ($LASTEXITCODE -ne 0) { throw "build failed" }
    New-Item -ItemType Directory -Force dist | Out-Null
    Copy-Item target/release/poro.exe dist/poro.exe -Force
    & ./target/release/poro-sign.exe dist/poro.exe
    if ($LASTEXITCODE -ne 0) { throw "signing failed (wrong release key?)" }
    Write-Host ""
    Write-Host "Ready: dist/poro.exe and dist/poro.exe.sig (v$version)."
    Write-Host "Publish with:"
    Write-Host "  gh release create v$version dist/poro.exe dist/poro.exe.sig --title `"Poro v$version`" --notes `"<what's new>`""
} finally { Pop-Location }
