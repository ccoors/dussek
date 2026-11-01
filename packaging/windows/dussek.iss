; Dussek's Windows installer (Inno Setup 6), made from the portable folder (plugin/tools/package.sh):
;   ISCC /DVersion=<version> /DSource=<dist\dussek-<version>-windows> packaging\windows\dussek.iss
; writes dist\dussek-<version>-windows-setup.exe. The standalone and each plugin format can be left
; out; the instruments always go to %PROGRAMDATA%\Dussek\instruments, where the plugin looks for
; them (pe::systemDataDirs), emptied first so an instrument dropped since does not linger. The
; player's own files (%APPDATA%\Dussek) are never touched.

#ifndef Version
  #error Pass /DVersion=<version>
#endif
#ifndef Source
  #error Pass /DSource=<the portable folder>
#endif

[Setup]
; fixed for good: an update installs over the version before it
AppId={{40BB77E5-0840-459D-A153-A9FDA3E28C08}
AppName=Dussek
AppVersion={#Version}
AppVerName=Dussek {#Version}
AppPublisher=Christian Friedrich Coors
AppPublisherURL=https://github.com/ccoors/dussek
AppSupportURL=https://github.com/ccoors/dussek/issues
DefaultDirName={autopf}\Dussek
; fixed: a plugin's Copy CLI looks for the standalone there (pe::installedProgramDirs)
DisableDirPage=yes
DefaultGroupName=Dussek
DisableProgramGroupPage=yes
PrivilegesRequired=admin
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
OutputDir=..\..\dist
OutputBaseFilename=dussek-{#Version}-windows-setup
SetupIconFile=..\icon\dussek.ico
UninstallDisplayIcon={app}\dussek.ico
UninstallDisplayName=Dussek
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern

[Messages]
WelcomeLabel2=This installs Dussek {#Version}: a physically modeled piano, harpsichords, harps and percussion, as a standalone and as LV2, CLAP and VST3 plugins.%n%nThe installer is not signed, which is why Windows warned about an unknown publisher.

[Types]
Name: "full"; Description: "Everything"
Name: "custom"; Description: "Choose"; Flags: iscustom

[Components]
Name: "standalone"; Description: "The standalone (in the Start menu)"; Types: full custom
Name: "lv2"; Description: "LV2 plugin"; Types: full custom
Name: "clap"; Description: "CLAP plugin"; Types: full custom
Name: "vst3"; Description: "VST3 plugin (where a host offers LV2 or CLAP, prefer them)"; Types: full custom

[InstallDelete]
; what an earlier version installed: replaced whole
Type: filesandordirs; Name: "{commonappdata}\Dussek\instruments"
Type: filesandordirs; Name: "{commoncf64}\LV2\dussek.lv2"
Type: filesandordirs; Name: "{commoncf64}\VST3\dussek.vst3"
Type: files; Name: "{commoncf64}\CLAP\dussek.clap"
Type: files; Name: "{app}\dussek.exe"
; the Start menu entry, gone with the standalone when an update leaves it out
Type: files; Name: "{autoprograms}\Dussek.lnk"

[Files]
Source: "{#Source}\dussek.exe"; DestDir: "{app}"; Components: standalone; Flags: ignoreversion
Source: "{#Source}\README.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Source}\LICENSE.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\icon\dussek.ico"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Source}\dussek.lv2\*"; DestDir: "{commoncf64}\LV2\dussek.lv2"; Components: lv2; Flags: ignoreversion recursesubdirs
Source: "{#Source}\dussek.clap"; DestDir: "{commoncf64}\CLAP"; Components: clap; Flags: ignoreversion
Source: "{#Source}\dussek.vst3\*"; DestDir: "{commoncf64}\VST3\dussek.vst3"; Components: vst3; Flags: ignoreversion recursesubdirs
Source: "{#Source}\instruments\*"; DestDir: "{commonappdata}\Dussek\instruments"; Flags: ignoreversion recursesubdirs

[Icons]
Name: "{autoprograms}\Dussek"; Filename: "{app}\dussek.exe"; IconFilename: "{app}\dussek.ico"; Components: standalone

[Run]
Filename: "{app}\dussek.exe"; Description: "Start Dussek"; Components: standalone; Flags: nowait postinstall skipifsilent

[UninstallDelete]
Type: filesandordirs; Name: "{commonappdata}\Dussek\instruments"
