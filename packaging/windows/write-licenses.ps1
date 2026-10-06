<#
.SYNOPSIS
  Writes the third-party notices of the staged Windows package.

.DESCRIPTION
  The notices are generated from what is actually in the staged tree, not from
  a fixed list:

  * every Qt6*.dll is looked up in the SPDX SBOMs that ship with the Qt kit
    (<Qt>\sbom\qtbase-*.spdx.json, qtdeclarative-*.spdx.json), which give the
    module's licence and the third-party code Qt bundles inside it;
  * the Microsoft C++ runtime DLLs in the tree are listed with their versions;
  * the licence text of every SPDX identifier used is copied from
    packaging/windows/licenses/ (the texts of Qt's own LICENSES/ directories,
    tag v6.8.3). A missing text stops the build.

  Output (inside the stage): licenses\THIRD_PARTY_NOTICES.md and
  licenses\texts\<SPDX-id>.txt. It is an inventory, not legal advice.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Stage,
    [Parameter(Mandatory)][string]$QtRoot,
    [Parameter(Mandatory)][string]$Repo
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'PackagingTools.ps1')

function Read-Sbom([string]$pattern) {
    $file = Get-ChildItem (Join-Path $QtRoot 'sbom') -Filter $pattern | Select-Object -First 1
    if (-not $file) { throw "Qt SBOM not found: $QtRoot\sbom\$pattern (needed for the licence inventory)" }
    Get-Content -Raw -Encoding UTF8 $file.FullName | ConvertFrom-Json
}
$base = Read-Sbom 'qtbase-*.spdx.json'
$decl = Read-Sbom 'qtdeclarative-*.spdx.json'

function Get-Module($sbom, [string]$repoName, [string]$name) {
    $id = "SPDXRef-Package-$repoName-qt-module-$name"
    $sbom.packages | Where-Object { $_.SPDXID -eq $id } | Select-Object -First 1
}
function Split-Licenses([string]$expression) {
    ($expression -split '[\s()]+' | Where-Object { $_ -and $_ -notin 'AND', 'OR', 'WITH' })
}

# ---- Qt modules present --------------------------------------------------------
$qtDlls = Get-ChildItem $Stage -File -Filter 'Qt6*.dll' | Sort-Object Name
$qtVersion = ($qtDlls | Where-Object Name -eq 'Qt6Core.dll' | Select-Object -First 1).VersionInfo.ProductVersion
if (-not $qtVersion) { throw 'Qt6Core.dll is not in the stage' }
$modules = @()
foreach ($dll in $qtDlls) {
    $name = $dll.BaseName.Substring(3)
    $pkg = Get-Module $base 'qtbase' $name
    $repoName = 'qtbase'
    if (-not $pkg) { $pkg = Get-Module $decl 'qtdeclarative' $name; $repoName = 'qtdeclarative' }
    if (-not $pkg) { throw "$($dll.Name) is not in Qt's SBOMs: the licence inventory cannot cover it" }
    $modules += [pscustomobject]@{ File = $dll.Name; Module = $name; Repo = $repoName; Version = $pkg.versionInfo; License = $pkg.licenseConcluded }
}
$qtBaseModules = $modules | Where-Object Repo -eq 'qtbase' | ForEach-Object Module

# ---- Third-party code Qt bundles in those modules ----------------------------------
# Bundled-library modules the present modules depend on (e.g. Gui -> FreeType).
$bundled = @()
foreach ($r in $base.relationships) {
    foreach ($m in $qtBaseModules) {
        if ($r.spdxElementId -eq "SPDXRef-Package-qtbase-qt-module-$m" -and $r.relationshipType -eq 'DEPENDS_ON' `
                -and $r.relatedSpdxElement -like '*qt-bundled-3rdparty-module-*') {
            $bundled += ($r.relatedSpdxElement -replace '.*qt-bundled-3rdparty-module-', '')
        }
    }
}
# FreeType depends on libpng; resolve one more level.
foreach ($r in $base.relationships) {
    if ($r.relationshipType -eq 'DEPENDS_ON' -and $r.relatedSpdxElement -like '*qt-bundled-3rdparty-module-*' `
            -and $r.spdxElementId -like '*qt-bundled-3rdparty-module-*') {
        $from = $r.spdxElementId -replace '.*qt-bundled-3rdparty-module-', ''
        if ($bundled -contains $from) { $bundled += ($r.relatedSpdxElement -replace '.*qt-bundled-3rdparty-module-', '') }
    }
}
$bundled = $bundled | Sort-Object -Unique
$prefixes = @($qtBaseModules) + @($bundled) + @('QWindowsIntegrationPlugin')

$third = @()
foreach ($p in $base.packages) {
    if ($p.SPDXID -match 'qt-bundled-3rdparty-module-(.+)$' -and $bundled -contains $Matches[1] -and $Matches[1] -notmatch 'Private$') {
        $third += [pscustomobject]@{ Name = $p.name; Version = $p.versionInfo; License = $p.licenseConcluded; Copyright = $p.copyrightText; Via = 'bundled in ' + (($qtBaseModules -join ', ')) }
    }
    elseif ($p.SPDXID -match 'qt-3rdparty-sources-([A-Za-z0-9]+)-Attribution-(.+)$' -and $prefixes -contains $Matches[1] -and $Matches[1] -notmatch 'Private$') {
        $third += [pscustomobject]@{ Name = $Matches[2]; Version = $p.versionInfo; License = $p.licenseConcluded; Copyright = $p.copyrightText; Via = "Qt$($Matches[1])" }
    }
}
foreach ($p in $decl.packages) {
    if ($p.SPDXID -match 'qt-3rdparty-sources-Qml-Attribution-(.+)$') {
        $third += [pscustomobject]@{ Name = $Matches[1]; Version = $p.versionInfo; License = $p.licenseConcluded; Copyright = $p.copyrightText; Via = 'QtQml' }
    }
}
$third = $third | Sort-Object Via, Name -Unique

# ---- Microsoft runtime files in the stage ------------------------------------------------
$crtNames = 'vcruntime140.dll', 'vcruntime140_1.dll', 'msvcp140.dll', 'msvcp140_1.dll', 'msvcp140_2.dll',
'msvcp140_atomic_wait.dll', 'msvcp140_codecvt_ids.dll', 'concrt140.dll', 'vccorlib140.dll', 'vcruntime140_threads.dll'
$crt = Get-ChildItem $Stage -File | Where-Object { $crtNames -contains $_.Name.ToLowerInvariant() } | Sort-Object Name

# ---- Licence texts ------------------------------------------------------------------------
$ids = @('LGPL-3.0-only', 'GPL-3.0-only')
foreach ($m in $modules) { $ids += Split-Licenses $m.License }
foreach ($t in $third) { $ids += Split-Licenses $t.License }
$ids = $ids | Where-Object { $_ -ne 'LicenseRef-Qt-Commercial' -and $_ -ne 'NOASSERTION' } | Sort-Object -Unique

$outDir = Join-Path $Stage 'licenses'
$textDir = Join-Path $outDir 'texts'
New-Item -ItemType Directory -Force $textDir | Out-Null
foreach ($id in $ids) {
    $src = Join-Path $PSScriptRoot "licenses\$id.txt"
    if (-not (Test-Path $src)) { throw "No licence text for $id in packaging\windows\licenses (needed by the Qt SBOM inventory)" }
    Copy-Item $src (Join-Path $textDir "$id.txt")
}

# ---- The notice ---------------------------------------------------------------------------------
$qtShort = ($qtVersion -split '\.')[0..2] -join '.'
$sb = New-Object System.Text.StringBuilder
function Add([string]$s = '') { [void]$sb.AppendLine($s) }
function Clean([string]$s) { ($s -replace '\s+', ' ').Trim() }

Add '# Third-party notices: OmaTree for Windows'
Add
Add 'This installer package contains OmaTree and the libraries listed below. The list is generated'
Add 'from the files that are actually in the package; it is an inventory, not legal advice.'
Add
Add '## OmaTree'
Add
Add 'OmaTree is licensed under either of the Apache License 2.0 (`LICENSE-APACHE`) or the MIT'
Add 'License (`LICENSE-MIT`), at your option. The Rust crates and SQLite compiled into `omatree.exe`'
Add 'are inventoried in `THIRD_PARTY.md`.'
Add
Add "## Qt $qtShort"
Add
Add "The package contains Qt $qtVersion, the official binary release for Windows (MSVC 2022, 64-bit) from"
Add '<https://download.qt.io/>, used under the **GNU LGPL version 3**. Qt is linked dynamically: every'
Add 'Qt library is a separate `Qt6*.dll` file next to `omatree.exe`, the Qt plug-ins and QML modules are'
Add 'separate files in `platforms\` and `qml\`, and nothing from Qt is compiled into `omatree.exe`.'
Add
Add '* **Replacing Qt.** You may replace these files with your own build of a compatible Qt 6.8 (same'
Add '  minor version, same modules); OmaTree loads whatever Qt it finds in its own folder.'
Add "* **Source.** The exact corresponding source of this Qt release is at <https://download.qt.io/archive/qt/$(($qtShort -split '\.')[0..1] -join '.')/$qtShort/>."
Add '* **Changes.** None: the Qt libraries, plug-ins and QML plug-ins are the official binaries, byte for'
Add '  byte (`windeployqt` only copies them; Qt for Windows locates its plug-ins relative to the application).'
Add '* **Unused Qt files were left out** (other Qt Quick Controls styles, image-format, TLS and debugging'
Add '  plug-ins, shader compilers); see `docs/PACKAGING.md` in the source for the evidence.'
Add '* The LGPL-3.0 text is `texts\LGPL-3.0-only.txt` (it builds on the GPL-3.0, `texts\GPL-3.0-only.txt`).'
Add
Add '### Qt modules in this package'
Add
Add '| File | Qt module | Version | Licence (as published in Qt''s SBOM) |'
Add '|---|---|---|---|'
foreach ($m in $modules) { Add "| ``$($m.File)`` | $($m.Module) | $($m.Version) | $($m.License -replace 'LicenseRef-Qt-Commercial OR ', '') |" }
Add "| ``platforms\qwindows.dll`` | QWindowsIntegrationPlugin | $qtShort | LGPL-3.0-only (also GPL options) |"
Add '| `qml\**\*.dll` and `qml\**\qmldir`, `*.qml` | Qt Quick / Qt Quick Controls / Qt Quick Dialogs / Layouts / Templates / Qml Models and WorkerScript / Labs FolderListModel (qtdeclarative) | ' + $qtShort + ' | LGPL-3.0-only (also GPL options) |'
Add
Add 'Qt is offered by The Qt Company under a commercial licence, LGPL-3.0, GPL-2.0 or GPL-3.0; this package'
Add 'uses it under the LGPL-3.0 (the commercial option is not used).'
Add
Add '### Third-party code that Qt''s SBOM lists for these modules'
Add
Add 'Qt''s own SBOM (SPDX files shipped with the Qt kit) lists this third-party code as bundled in, or'
Add 'attributed to, the Qt modules above. Some items apply to other platforms, or are header-only or'
Add 'build-time, and may not be compiled into the Windows binaries; the list errs towards including them.'
Add
Add '| Component | Version | Licence | Copyright | Listed under |'
Add '|---|---|---|---|---|'
foreach ($t in $third) {
    $c = Clean $t.Copyright
    if ($c.Length -gt 110) { $c = $c.Substring(0, 107) + '...' }
    $v = if ($t.Version) { Clean $t.Version } else { '' }
    if ($v.Length -gt 14) { $v = $v.Substring(0, 12) + '..' }
    Add "| $($t.Name) | $v | $($t.License) | $c | $($t.Via) |"
}
Add
Add '## Microsoft Visual C++ runtime'
Add
if ($crt) {
    Add 'These files are copied unmodified from the Microsoft Visual C++ Redistributable files of the'
    Add 'Visual Studio 2022 Build Tools used to build OmaTree (`VC\Redist\MSVC\<version>\x64\Microsoft.VC143.CRT`),'
    Add 'and are installed next to `omatree.exe` (app-local deployment), so that no separate installation of'
    Add 'the Visual C++ Redistributable is needed. They are redistributed under Microsoft''s terms for'
    Add 'redistributable code; see <https://learn.microsoft.com/cpp/windows/determining-which-dlls-to-redistribute>.'
    Add 'The Universal C Runtime (`api-ms-win-crt-*`, `ucrtbase.dll`) is part of Windows 10 and later and is not'
    Add 'shipped.'
    Add
    Add '| File | Version |'
    Add '|---|---|'
    foreach ($f in $crt) { Add "| ``$($f.Name)`` | $($f.VersionInfo.FileVersion) |" }
} else {
    Add 'No Microsoft runtime files are in this package.'
}
Add
Add '## Licence texts'
Add
Add 'The licence texts are in `texts\` (the SPDX identifier is the file name). They are the texts of Qt''s'
Add 'own `LICENSES` directories, tag v6.8.3:'
Add
Add (($ids | ForEach-Object { "* ``$_``" }) -join "`r`n")
[IO.File]::WriteAllText((Join-Path $outDir 'THIRD_PARTY_NOTICES.md'), $sb.ToString(), (New-Object Text.UTF8Encoding $false))

"Notices: $($modules.Count) Qt modules, $($third.Count) bundled third-party entries, $($crt.Count) Microsoft runtime files, $($ids.Count) licence texts"
