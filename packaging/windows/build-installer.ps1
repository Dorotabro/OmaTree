<#
.SYNOPSIS
  Builds the OmaTree Windows installer: dist\OmaTree-<version>-x64-setup.exe

.DESCRIPTION
  1. reads the version from Cargo.toml (the only place it is written)
  2. builds the release executable into target\windows, with the checkout, Cargo
     and Qt paths removed from the binary (a normal `cargo build` is untouched)
  3. stages the application in target\windows-package\app: the executable, the
     Qt runtime from `windeployqt`, the Microsoft C++ runtime DLLs the files
     really import (app-local), the evidence-based prune (prune.txt), licences
  4. verifies the stage (resources, dependencies, inventory): verify-package.ps1
  5. compiles the Inno Setup installer and verifies it
  6. writes a SHA-256 sidecar next to it

  Nothing is signed, tagged, uploaded or published. Needs Windows 10 or later,
  64-bit; Rust (x86_64-pc-windows-msvc); Visual Studio 2022 Build Tools (C++
  workload and Windows SDK); a Qt 6.8 msvc2022_64 kit with windeployqt and the
  sbom folder; Inno Setup 6.3 or later. Each tool is found automatically or
  given explicitly.

.PARAMETER QtDir      Qt kit root (the folder with bin\windeployqt.exe).
                      Default: $env:QT_DIR, the folder of $env:QMAKE, windeployqt on PATH, C:\Qt\*\msvc*_64.
.PARAMETER InnoSetup  Path to ISCC.exe. Default: $env:ISCC, PATH, the usual Inno Setup 6 folders.
.PARAMETER VcRedistDir  The Microsoft.VC143.CRT folder to take the C++ runtime DLLs from.
                      Default: found through the Visual Studio developer environment.
.PARAMETER OutDir     Where the installer and its .sha256 are written. Default: dist\ (git-ignored).
.PARAMETER SkipBuild  Reuse target\windows\release\omatree.exe instead of building.
#>
[CmdletBinding()]
param(
    [string]$QtDir,
    [string]$InnoSetup,
    [string]$VcRedistDir,
    [string]$OutDir,
    [switch]$SkipBuild
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'PackagingTools.ps1')

$repo = Get-RepoRoot
$version = Get-CargoVersion $repo
if (-not $OutDir) { $OutDir = Join-Path $repo 'dist' }
$stage = Join-Path $repo 'target\windows-package\app'
$buildTarget = Join-Path $repo 'target\windows'

function Write-Step([string]$text) { Write-Host "`n== $text" -ForegroundColor Cyan }
function Fail([string]$text) { throw $text }

if ([Environment]::OSVersion.Platform -ne 'Win32NT') { Fail 'This script builds the Windows installer and runs on Windows.' }
if (-not [Environment]::Is64BitOperatingSystem) { Fail 'A 64-bit Windows is required.' }

# ---- Tools ---------------------------------------------------------------------------------------
function Find-QtKit {
    $candidates = @()
    if ($QtDir) { $candidates += $QtDir }
    if ($env:QT_DIR) { $candidates += $env:QT_DIR }
    if ($env:QMAKE) { $candidates += (Split-Path -Parent (Split-Path -Parent $env:QMAKE)) }
    $onPath = Get-Command windeployqt.exe -ErrorAction SilentlyContinue
    if ($onPath) { $candidates += (Split-Path -Parent (Split-Path -Parent $onPath.Source)) }
    $candidates += (Get-ChildItem 'C:\Qt\*\msvc*_64' -Directory -ErrorAction SilentlyContinue | Sort-Object FullName -Descending | ForEach-Object FullName)
    foreach ($c in $candidates) {
        if ($c -and (Test-Path (Join-Path $c 'bin\windeployqt.exe'))) { return (Resolve-Path $c).Path }
    }
    Fail 'Qt kit not found. Pass -QtDir <Qt>\6.8.x\msvc2022_64 (the folder that contains bin\windeployqt.exe).'
}
function Find-Iscc {
    $candidates = @()
    if ($InnoSetup) { $candidates += $InnoSetup }
    if ($env:ISCC) { $candidates += $env:ISCC }
    $onPath = Get-Command ISCC.exe -ErrorAction SilentlyContinue
    if ($onPath) { $candidates += $onPath.Source }
    foreach ($root in ${env:ProgramFiles(x86)}, $env:ProgramFiles, "$env:LOCALAPPDATA\Programs") {
        if ($root) { $candidates += (Join-Path $root 'Inno Setup 6\ISCC.exe') }
    }
    foreach ($c in $candidates) { if ($c -and (Test-Path $c)) { return (Resolve-Path $c).Path } }
    Fail 'Inno Setup 6 not found. Install it (winget install JRSoftware.InnoSetup) or pass -InnoSetup <path>\ISCC.exe.'
}
function Enter-MsvcEnvironment {
    if (Get-Command cl.exe -ErrorAction SilentlyContinue) { return }
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (-not (Test-Path $vswhere)) { Fail 'Visual Studio 2022 Build Tools (C++ workload) not found; install them or run from a Developer PowerShell.' }
    $vs = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if (-not $vs) { Fail 'No Visual Studio installation with the C++ x64 tools was found.' }
    $bat = Join-Path $vs 'VC\Auxiliary\Build\vcvars64.bat'
    cmd /c "`"$bat`" >nul 2>&1 && set" | ForEach-Object {
        if ($_ -match '^([^=]+)=(.*)$') { Set-Item -Path "env:$($Matches[1])" -Value $Matches[2] }
    }
    if (-not (Get-Command cl.exe -ErrorAction SilentlyContinue)) { Fail 'The MSVC environment could not be loaded (vcvars64.bat).' }
}
function Find-VcRedist {
    $candidates = @()
    if ($VcRedistDir) { $candidates += $VcRedistDir }
    if ($env:VCToolsRedistDir) { $candidates += (Get-ChildItem (Join-Path $env:VCToolsRedistDir 'x64') -Directory -Filter 'Microsoft.VC*.CRT' -ErrorAction SilentlyContinue | ForEach-Object FullName) }
    foreach ($c in $candidates) {
        if ($c -and (Test-Path (Join-Path $c 'vcruntime140.dll'))) { return (Resolve-Path $c).Path }
    }
    Fail 'The Microsoft.VC143.CRT redistributable folder was not found (VC\Redist\MSVC\<version>\x64\Microsoft.VC143.CRT). Pass -VcRedistDir.'
}

Write-Step "OmaTree $version (from Cargo.toml)"
Enter-MsvcEnvironment
$qt = Find-QtKit
$iscc = Find-Iscc
$crtDir = Find-VcRedist
$windeployqt = Join-Path $qt 'bin\windeployqt.exe'
Write-Host "Qt kit        : $qt"
Write-Host "Inno Setup    : $iscc"
Write-Host "C++ runtime   : $crtDir"
Write-Host "Output        : $OutDir"

# ---- 1. Release build --------------------------------------------------------------------------
$exe = Join-Path $buildTarget 'release\omatree.exe'
if ($SkipBuild) {
    if (-not (Test-Path $exe)) { Fail "-SkipBuild given but $exe does not exist." }
} else {
    Write-Step 'Release build (target\windows)'
    $cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE '.cargo' }
    $rustupHome = if ($env:RUSTUP_HOME) { $env:RUSTUP_HOME } else { Join-Path $env:USERPROFILE '.rustup' }
    $env:CARGO_TARGET_DIR = $buildTarget
    # cxx-qt-build finds Qt through qmake: the kit chosen above, not whatever the shell has.
    $env:QMAKE = Join-Path $qt 'bin\qmake.exe'
    # Neither the checkout, Cargo, rustup nor the PDB location may remain in the executable.
    $flags = @("--remap-path-prefix=$repo=omatree", "--remap-path-prefix=$cargoHome=cargo",
        "--remap-path-prefix=$rustupHome=rustup", '-C', 'link-arg=/PDBALTPATH:%_PDB%')
    $env:CARGO_ENCODED_RUSTFLAGS = ($flags -join [string][char]0x1f)
    $env:CXXFLAGS = "/d1trimfile:$repo\ /d1trimfile:$qt\ /d1trimfile:$cargoHome\"
    Push-Location $repo
    try { & cargo build --release --locked; if ($LASTEXITCODE -ne 0) { Fail 'cargo build failed.' } }
    finally { Pop-Location; Remove-Item Env:\CARGO_TARGET_DIR, Env:\CARGO_ENCODED_RUSTFLAGS, Env:\CXXFLAGS, Env:\QMAKE -ErrorAction SilentlyContinue }
}

# ---- 2. Stage ---------------------------------------------------------------------------------------
Write-Step "Staging the application ($stage)"
if (Test-Path $stage) { Remove-Item -LiteralPath $stage -Recurse -Force }
New-Item -ItemType Directory -Force $stage | Out-Null
Copy-Item $exe $stage

# Qt, from the release executable and the QML it imports. Only what the evidence
# in docs/PACKAGING.md supports is skipped here; the rest is pruned from prune.txt.
Write-Step 'windeployqt'
$wdq = @('--release', '--qmldir', (Join-Path $repo 'qml'), '--no-translations', '--no-system-d3d-compiler',
    '--no-system-dxc-compiler', '--no-compiler-runtime', '--no-opengl-sw',
    '--skip-plugin-types', 'generic,iconengines,imageformats,networkinformation,qmltooling,tls',
    '--dir', $stage, (Join-Path $stage 'omatree.exe'))
# (It writes warnings to stderr, e.g. about translation catalogs a kit may not
# have even with --no-translations; the exit code is what counts.)
$previous = $ErrorActionPreference
$ErrorActionPreference = 'Continue'
$log = & $windeployqt @wdq 2>&1
$deployExit = $LASTEXITCODE
$ErrorActionPreference = $previous
if ($deployExit -ne 0) { $log | Write-Host; Fail 'windeployqt failed.' }

# What windeployqt copied must be the kit's files, byte for byte (the licence
# notice says so).
foreach ($f in Get-ChildItem $stage -Recurse -File -Filter *.dll) {
    $rel = $f.FullName.Substring($stage.Length + 1)
    $origin = if ($rel -like 'Qt6*') { Join-Path $qt "bin\$rel" } elseif ($rel -like 'platforms\*') { Join-Path $qt "plugins\$rel" } else { Join-Path $qt $rel }
    if (-not (Test-Path $origin)) { Fail "$rel has no counterpart in the Qt kit ($origin)." }
    if ((Get-FileHash $f.FullName).Hash -ne (Get-FileHash $origin).Hash) { Fail "$rel differs from the Qt kit's file." }
}

# The prune, with its evidence in prune.txt.
Write-Step 'Pruning (prune.txt)'
foreach ($line in Get-Content (Join-Path $PSScriptRoot 'prune.txt')) {
    $entry = ($line -replace '#.*$', '').Trim()
    if (-not $entry) { continue }
    $matched = @(Get-Item -Path (Join-Path $stage $entry) -Force -ErrorAction SilentlyContinue)
    if ($matched.Count -eq 0) { Write-Warning "prune.txt: nothing matches '$entry'"; continue }
    $matched | Remove-Item -Recurse -Force
}

# The Microsoft C++ runtime, app-local: exactly the redistributable DLLs that
# the staged files import (and that they in turn import), no more.
Write-Step 'Microsoft C++ runtime (app-local)'
do {
    $added = $false
    foreach ($pe in Get-PeFiles $stage) {
        foreach ($dll in [PeTools]::Imports($pe.FullName)) {
            $source = Join-Path $crtDir $dll
            if (-not (Test-Path (Join-Path $stage $dll)) -and (Test-Path $source)) {
                Copy-Item $source $stage
                Write-Host "  + $dll ($((Get-Item $source).VersionInfo.FileVersion))"
                $added = $true
            }
        }
    }
} while ($added)

# Licences and the notices of exactly this tree.
Write-Step 'Licences'
foreach ($doc in 'LICENSE-MIT', 'LICENSE-APACHE', 'THIRD_PARTY.md') { Copy-Item (Join-Path $repo $doc) $stage }
& (Join-Path $PSScriptRoot 'write-licenses.ps1') -Stage $stage -QtRoot $qt -Repo $repo

# ---- 3. Verify the stage ----------------------------------------------------------------------------
Write-Step 'Verifying the staged application'
& (Join-Path $PSScriptRoot 'verify-package.ps1') -Stage $stage
if ($LASTEXITCODE -ne 0) { Fail 'The staged application failed verification.' }

# ---- 4. Installer -----------------------------------------------------------------------------------------
Write-Step 'Inno Setup'
New-Item -ItemType Directory -Force $OutDir | Out-Null
$numeric = ((($version -replace '[-+].*$', '') -split '\.') + '0' + '0' + '0')[0..3] -join '.'
$issArgs = @('/Q', "/DAppVersion=$version", "/DAppVersionNumeric=$numeric", "/DStageDir=$stage",
    "/DRepoDir=$repo", "/DOutputDir=$OutDir", (Join-Path $PSScriptRoot 'OmaTree.iss'))
& $iscc @issArgs
if ($LASTEXITCODE -ne 0) { Fail 'Inno Setup failed.' }
$setup = Join-Path $OutDir "OmaTree-$version-x64-setup.exe"
if (-not (Test-Path $setup)) { Fail "Expected installer not produced: $setup" }

# ---- 5. Checksum -------------------------------------------------------------------------------------------
Write-Step 'Checksum'
$hash = (Get-FileHash -Algorithm SHA256 $setup).Hash.ToLowerInvariant()
$sidecar = "$setup.sha256"
[IO.File]::WriteAllText($sidecar, "$hash  $(Split-Path -Leaf $setup)`n", (New-Object Text.UTF8Encoding $false))
& (Join-Path $PSScriptRoot 'verify-package.ps1') -Installer $setup
if ($LASTEXITCODE -ne 0) { Fail 'The installer failed verification.' }

Write-Step 'Done'
Write-Host ("{0}  ({1:N1} MB)" -f $setup, ((Get-Item $setup).Length / 1MB))
Write-Host "$sidecar"
Write-Host "sha256 $hash"
Write-Host 'Unsigned; nothing was tagged or published.'
