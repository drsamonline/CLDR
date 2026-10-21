; =============================================================================
; CLDR — Command Line Dispatch & Route  (Windows installer, Inno Setup 6)
; © 2026 Dr. Sohil Momin (drsamonline). MIT-licensed: retain this notice.
; =============================================================================
; Built in CI with:
;   ISCC.exe /DMyAppVersion=v1.3.0 /DSourceDir=...\pkg\cldr-windows-x86_64 ^
;            /DOutputDir=...\dist packaging\cldr.iss

#ifndef MyAppVersion
  #define MyAppVersion "1.3.0"
#endif
#ifndef SourceDir
  #define SourceDir ".."
#endif
#ifndef OutputDir
  #define OutputDir ".."
#endif

[Setup]
AppId={{8F1C2B7A-3D4E-4B9A-9C1E-7A2B4C6D8E01}
AppName=CLDR
AppVersion={#MyAppVersion}
AppVerName=CLDR {#MyAppVersion}
AppPublisher=Dr. Sohil Momin
AppPublisherURL=https://github.com/drsamonline/CLDR
AppSupportURL=https://github.com/drsamonline/CLDR/issues
DefaultDirName={autopf}\CLDR
DefaultGroupName=CLDR
DisableProgramGroupPage=yes
OutputDir={#OutputDir}
OutputBaseFilename=cldr-{#MyAppVersion}-setup
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
UninstallDisplayName=CLDR — Command Line Dispatch & Route
UninstallDisplayIcon={app}\cldr.exe
ArchitecturesInstallIn64BitMode=x64compatible

[Files]
Source: "{#SourceDir}\cldr.exe";   DestDir: "{app}";        Flags: ignoreversion
Source: "{#SourceDir}\README.md";  DestDir: "{app}";        Flags: ignoreversion
Source: "{#SourceDir}\LICENSE";    DestDir: "{app}";        Flags: ignoreversion
Source: "{#SourceDir}\CHANGELOG.md"; DestDir: "{app}";      Flags: ignoreversion
Source: "{#SourceDir}\docs\*";     DestDir: "{app}\docs";   Flags: ignoreversion
Source: "{#SourceDir}\scripts\*";  DestDir: "{app}\scripts"; Flags: ignoreversion

[Icons]
Name: "{group}\CLDR";            Filename: "{app}\cldr.exe"
Name: "{group}\Summon CLDR";     Filename: "{app}\cldr.exe"; Parameters: "--summon"
Name: "{group}\Uninstall CLDR";  Filename: "{uninstallexe}"
Name: "{autodesktop}\CLDR";      Filename: "{app}\cldr.exe"
; Background tray resident at logon (hidden window).
Name: "{userstartup}\CLDR Tray"; Filename: "{app}\cldr.exe"; Parameters: "--tray"

[Run]
Filename: "{app}\cldr.exe"; Parameters: "--config"; Flags: runhidden; \
  Description: "Create the starter config file"
Filename: "{app}\cldr.exe"; Description: "Launch CLDR now"; \
  Flags: nowait postinstall skipifsilent

[UninstallRun]
Filename: "{app}\cldr.exe"; Parameters: "--daemon-stop"; Flags: runhidden; RunOnceId: "StopDaemon"
