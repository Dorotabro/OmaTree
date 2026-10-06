<#
.SYNOPSIS
  Verifies the staged Windows application or the finished installer.

  verify-package.ps1 -Stage <dir>       the staged application tree
  verify-package.ps1 -Installer <exe>   the installer and its checksum sidecar

  Exits 0 when everything passes, 1 otherwise, naming every failure.
#>
[CmdletBinding()]
param(
    [string]$Stage,
    [string]$Installer
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'PackagingTools.ps1')

$repo = Get-RepoRoot
$version = Get-CargoVersion $repo
$failures = New-Object System.Collections.Generic.List[string]
function Check([string]$what, [bool]$ok, [string]$detail = '') {
    if ($ok) { Write-Host "  ok    $what" } else { Write-Host "  FAIL  $what $detail" -ForegroundColor Red; $failures.Add($what + ' ' + $detail) }
}

if ($Stage) {
    $root = (Resolve-Path $Stage).Path
    Write-Host "Stage: $root"
    $exe = Join-Path $root 'omatree.exe'
    Check 'omatree.exe exists' (Test-Path $exe)
    if (Test-Path $exe) {
        # -- Resources ----------------------------------------------------------------
        Check 'GUI subsystem (no console window)' ([PeTools]::Subsystem($exe) -eq 2)
        Check 'exactly one manifest resource' ([PeTools]::ResourceCount($exe, $RT_MANIFEST) -eq 1) "(found $([PeTools]::ResourceCount($exe, $RT_MANIFEST)))"
        Check 'UTF-8 active code page in the manifest' ([PeTools]::Contains($exe, '<activeCodePage xmlns="http://schemas.microsoft.com/SMI/2019/WindowsSettings">UTF-8</activeCodePage>'))
        Check 'application icon resource' ([PeTools]::ResourceCount($exe, $RT_ICON_GROUP) -ge 1)
        Check 'version resource' ([PeTools]::ResourceCount($exe, $RT_VERSION) -eq 1)
        $vi = (Get-Item $exe).VersionInfo
        Check 'ProductName is OmaTree' ($vi.ProductName -eq 'OmaTree') "($($vi.ProductName))"
        Check 'FileDescription is OmaTree' ($vi.FileDescription -eq 'OmaTree') "($($vi.FileDescription))"
        Check "ProductVersion is the Cargo version $version" ($vi.ProductVersion -eq $version) "($($vi.ProductVersion))"
        Check "FileVersion is the Cargo version $version" ($vi.FileVersion -eq $version) "($($vi.FileVersion))"
        Check 'OriginalFilename is omatree.exe' ($vi.OriginalFilename -eq 'omatree.exe')
        Check 'copyright is stated' ($vi.LegalCopyright -like 'Copyright*OmaTree contributors*')

        # -- No build-machine paths in the executable ---------------------------------------------
        $forbidden = @('C:\Users\', 'C:/Users/', 'C:\Qt', 'C:/Qt', '.cargo', '.rustup', 'BuildTools', 'Visual Studio', $repo, ($repo -replace '\\', '/'), $env:USERNAME) |
            Where-Object { $_ } | Select-Object -Unique
        $found = @($forbidden | Where-Object { [PeTools]::Contains($exe, $_) })
        Check 'no developer paths or names in omatree.exe' ($found.Count -eq 0) "(found: $($found -join ', '))"
    }
    foreach ($f in Get-ChildItem $root -File -Filter 'Qt6*.dll') {
        if ([PeTools]::Contains($f.FullName, 'C:/Qt') -or [PeTools]::Contains($f.FullName, 'C:\Qt')) { $failures.Add("$($f.Name) contains a C:\Qt path"); Write-Host "  FAIL  $($f.Name) contains a C:\Qt path" -ForegroundColor Red }
    }

    # -- Required content ----------------------------------------------------------------------------------
    $required = 'Qt6Core.dll', 'Qt6Gui.dll', 'Qt6Qml.dll', 'Qt6Quick.dll', 'Qt6QuickControls2.dll', 'Qt6QuickControls2Basic.dll',
    'Qt6QuickTemplates2.dll', 'Qt6QuickLayouts.dll', 'Qt6QuickDialogs2.dll', 'platforms\qwindows.dll',
    'qml\QtQuick\qmldir', 'qml\QtQuick\Controls\qmldir', 'qml\QtQuick\Controls\Basic\qmldir', 'qml\QtQuick\Layouts\qmldir',
    'qml\QtQuick\Dialogs\qmldir', 'qml\QtQuick\Templates\qmldir', 'qml\QtQuick\Window\qmldir',
    'vcruntime140.dll', 'msvcp140.dll',
    'LICENSE-MIT', 'LICENSE-APACHE', 'THIRD_PARTY.md', 'licenses\THIRD_PARTY_NOTICES.md', 'licenses\texts\LGPL-3.0-only.txt'
    $missing = @($required | Where-Object { -not (Test-Path (Join-Path $root $_)) })
    Check 'required runtime files are present' ($missing.Count -eq 0) "(missing: $($missing -join ', '))"

    # -- Nothing that must not ship ------------------------------------------------------------------------------
    $bad = @(Get-ChildItem $root -Recurse -File | Where-Object {
            ($_.Extension -in '.lib', '.pdb', '.exp', '.obj', '.ilk', '.h', '.cpp', '.prl', '.cmake') `
                -or ($_.Name -like 'vc_redist*') `
                -or ($_.Name -match '(^Qt6.+d|plugind|^qwindowsd)\.dll$') } |
        ForEach-Object { $_.FullName.Substring($root.Length + 1) })
    Check 'no import libraries, symbols, headers, debug Qt or redistributable installers' ($bad.Count -eq 0) "($($bad -join ', '))"

    # -- Pruned items are really gone -----------------------------------------------------------------------------
    $stillThere = @()
    foreach ($line in Get-Content (Join-Path $PSScriptRoot 'prune.txt')) {
        $entry = ($line -replace '#.*$', '').Trim()
        if ($entry -and (Test-Path (Join-Path $root $entry))) { $stillThere += $entry }
    }
    Check 'everything in prune.txt is absent' ($stillThere.Count -eq 0) "($($stillThere -join ', '))"

    # -- Dependency audit ---------------------------------------------------------------------------------------------
    $problems = @(Test-Dependencies $root)
    Check 'every DLL dependency is in the application folder or a Windows system DLL' ($problems.Count -eq 0)
    $problems | ForEach-Object { Write-Host "          $_" -ForegroundColor Red }

    # -- Licence inventory covers the tree ------------------------------------------------------------------------------
    $notices = Get-Content -Raw (Join-Path $root 'licenses\THIRD_PARTY_NOTICES.md')
    $uncovered = @()
    foreach ($f in Get-ChildItem $root -Recurse -File) {
        $rel = $f.FullName.Substring($root.Length + 1)
        if ($rel -eq 'omatree.exe' -or $rel -like 'licenses\*' -or $rel -in 'LICENSE-MIT', 'LICENSE-APACHE', 'THIRD_PARTY.md') { continue }
        if ($rel -like 'qml\*' -or $rel -eq 'platforms\qwindows.dll') { continue }           # covered by their rows
        if ($f.Name -like 'Qt6*.dll' -or $f.Name -match '^(vcruntime|msvcp|concrt|vccorlib)') {
            if ($notices -notmatch [regex]::Escape($f.Name)) { $uncovered += $rel }
            continue
        }
        $uncovered += $rel
    }
    Check 'every bundled component is in the licence inventory' ($uncovered.Count -eq 0) "(not covered: $($uncovered -join ', '))"
    $ids = [regex]::Matches($notices, '(?m)^\* `([^`]+)`
?$') | ForEach-Object { $_.Groups[1].Value }
    $noText = @($ids | Where-Object { -not (Test-Path (Join-Path $root "licenses\texts\$_.txt")) })
    Check 'every listed licence has its text' ($noText.Count -eq 0 -and $ids.Count -gt 0) "(missing: $($noText -join ', '))"
}

if ($Installer) {
    $setup = (Resolve-Path $Installer).Path
    Write-Host "Installer: $setup"
    Check "file name is OmaTree-$version-x64-setup.exe" ((Split-Path -Leaf $setup) -eq "OmaTree-$version-x64-setup.exe")
    Check 'GUI subsystem' ([PeTools]::Subsystem($setup) -eq 2)
    $vi = (Get-Item $setup).VersionInfo
    Check "installer version information is $version" ($vi.ProductVersion.Trim() -eq $version) "($($vi.ProductVersion))"
    # (Inno Setup's setup program is a 32-bit executable; the application it installs is 64-bit,
    # and ArchitecturesAllowed=x64compatible in OmaTree.iss makes it refuse to run elsewhere.)
    $sidecar = "$setup.sha256"
    Check 'SHA-256 sidecar exists' (Test-Path $sidecar)
    if (Test-Path $sidecar) {
        $line = (Get-Content $sidecar | Select-Object -First 1)
        $expected = (Get-FileHash -Algorithm SHA256 $setup).Hash.ToLowerInvariant()
        Check 'SHA-256 sidecar matches ("<hash>  <file>")' ($line -eq "$expected  $(Split-Path -Leaf $setup)")
    }
}

if (-not $Stage -and -not $Installer) { Write-Host 'Pass -Stage <dir> or -Installer <exe>.'; exit 2 }
if ($failures.Count -gt 0) { Write-Host "`n$($failures.Count) check(s) failed." -ForegroundColor Red; exit 1 }
Write-Host "`nAll checks passed."
exit 0
