; OmaTree for Windows: per-user, x64 installer (Inno Setup 6).
;
; Not meant to be compiled by hand: build-installer.ps1 stages the application
; and passes everything that varies:
;   /DAppVersion=0.1.1          from Cargo.toml (so a version bump needs no edit here)
;   /DAppVersionNumeric=0.1.1.0 the same, as four numbers, for version resources
;   /DStageDir=...              the staged application tree
;   /DRepoDir=...               the repository root
;   /DOutputDir=...             where the setup program is written
; Optional, for signing later: /DSignToolName=<name> with the tool registered
; to ISCC by /S<name>=<command>; nothing is signed otherwise.

#ifndef AppVersion
  #error AppVersion is not defined: run packaging\windows\build-installer.ps1
#endif
#ifndef AppVersionNumeric
  #define AppVersionNumeric AppVersion + ".0"
#endif
#ifndef StageDir
  #error StageDir is not defined: run packaging\windows\build-installer.ps1
#endif
#ifndef RepoDir
  #error RepoDir is not defined: run packaging\windows\build-installer.ps1
#endif
#ifndef OutputDir
  #define OutputDir RepoDir + "\dist"
#endif

#define AppName "OmaTree"
#define AppExe "omatree.exe"
; The Windows AppUserModelID: the application id the Linux desktop file and the
; macOS bundle also use. Must match DESKTOP_FILE_NAME in src/main.rs.
#define AppUserModelId "io.github.Dorotabro.OmaTree"
; The ProgID of .omatree notebooks.
#define ProgId "OmaTree.Notebook"

[Setup]
; The AppId identifies this product to every future installer. NEVER change it:
; a different AppId would make an upgrade install side by side instead of over
; the old version. (The doubled brace is Inno's escape for a literal "{".)
AppId={{60910A77-7C66-4B5D-ABCB-3D8A4D819908}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher=OmaTree contributors
AppPublisherURL=https://github.com/Dorotabro/OmaTree
AppSupportURL=https://github.com/Dorotabro/OmaTree/issues
AppCopyright=Copyright (c) 2026 OmaTree contributors
VersionInfoVersion={#AppVersionNumeric}
VersionInfoProductName={#AppName}
VersionInfoProductVersion={#AppVersion}
VersionInfoDescription={#AppName} Setup
VersionInfoCopyright=Copyright (c) 2026 OmaTree contributors

; Per user: no administrator rights, no UAC prompt. {autopf} is then
; %LOCALAPPDATA%\Programs, {autoprograms} the user's Start menu, and HKA the
; user's registry hive.
PrivilegesRequired=lowest
DefaultDirName={autopf}\{#AppName}
DisableProgramGroupPage=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; Windows 10 1809: what Qt 6.8 itself requires.
MinVersion=10.0.17763

OutputDir={#OutputDir}
OutputBaseFilename={#AppName}-{#AppVersion}-x64-setup
SetupIconFile={#RepoDir}\windows\omatree.ico
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#AppName}
WizardStyle=modern
Compression=lzma2/max
SolidCompression=yes

; The .omatree association changes the shell: tell Explorer when installing and
; removing.
ChangesAssociations=yes
; A running OmaTree locks its files: ask to close it (its own unsaved-changes
; question applies) but do not start it again by itself.
CloseApplications=yes
RestartApplications=no

#ifdef SignToolName
SignTool={#SignToolName}
SignedUninstaller=yes
#endif

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
; Opt-in only.
Name: "desktopicon"; Description: "Create a &desktop shortcut"; GroupDescription: "Shortcuts:"; Flags: unchecked

[Files]
Source: "{#StageDir}\*"; DestDir: "{app}"; Flags: recursesubdirs createallsubdirs ignoreversion

[Icons]
; The AppUserModelID ties the shortcut (and a taskbar pin made from it) to the
; running program, which sets the same id itself (cpp/app_identity.cpp).
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExe}"; AppUserModelID: "{#AppUserModelId}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; AppUserModelID: "{#AppUserModelId}"; Tasks: desktopicon

[Registry]
; The .omatree file type: a ProgID, so that Explorer shows "OmaTree Notebook"
; and the right icon and opens it with the quoted command below. Only .omatree
; is registered (never a generic SQLite or database type). Written for the
; user (HKCU\Software\Classes) and removed again by the uninstaller; a different
; program the user chose for .omatree (their own UserChoice) is not touched.
Root: HKA; Subkey: "Software\Classes\.omatree"; ValueType: string; ValueName: ""; ValueData: "{#ProgId}"; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\.omatree"; ValueType: string; ValueName: "Content Type"; ValueData: "application/x-omatree"; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\.omatree\OpenWithProgids"; ValueType: string; ValueName: "{#ProgId}"; ValueData: ""; Flags: uninsdeletevalue uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\{#ProgId}"; ValueType: string; ValueName: ""; ValueData: "OmaTree Notebook"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\{#ProgId}"; ValueType: string; ValueName: "AppUserModelID"; ValueData: "{#AppUserModelId}"
Root: HKA; Subkey: "Software\Classes\{#ProgId}\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"",0"
Root: HKA; Subkey: "Software\Classes\{#ProgId}\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""

[Run]
Filename: "{app}\{#AppExe}"; Description: "Launch {#AppName}"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
; The only thing OmaTree itself writes outside a notebook: Qt's caches (a compiled
; QML cache and the Qt Quick graphics pipeline cache, a few KB, rebuilt on demand)
; in %LOCALAPPDATA%\OmaTree\cache. Removed with the program. Nothing else is
; deleted: the uninstaller removes only what the installer put in {app}, and
; notebooks are ordinary files elsewhere that are never touched.
Type: filesandordirs; Name: "{localappdata}\{#AppName}"
