; Dussek's Windows installer (Inno Setup 6), Weidinger in it as a component, made from the portable
; folder (plugin/tools/package.sh) with the brass moved apart (packaging/split-brass.sh: instruments-brass):
;   ISCC /DVersion=<version> /DSource=<the staged folder> packaging\windows\dussek.iss
; writes dist\dussek-<version>-windows-setup.exe. The standalone and each plugin format can be left
; out, and Weidinger (the brass, a preview: its standalone and the formats chosen, and its instruments);
; the instruments go to %PROGRAMDATA%\Dussek\instruments, where the plugins look for them
; (pe::systemDataDirs), emptied first so an instrument dropped since does not linger. The player's own
; files (%APPDATA%\Dussek) are never touched.

#ifndef Version
  #error Pass /DVersion=<version>
#endif
#ifndef Source
  #error Pass /DSource=<the staged folder: the portable one, the brass in instruments-brass>
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
WelcomeLabel2=This installs Dussek {#Version}: a physically modeled piano, harpsichords, harps and percussion, as a standalone and as LV2, CLAP and VST3 plugins; and Weidinger beside it, the brass (a preview; it can be left out).%n%nThe installer is not signed, which is why Windows warned about an unknown publisher.

[Types]
Name: "full"; Description: "Everything"
Name: "custom"; Description: "Choose"; Flags: iscustom

[Components]
Name: "standalone"; Description: "The standalone (in the Start menu)"; Types: full custom
Name: "lv2"; Description: "LV2 plugin"; Types: full custom
Name: "clap"; Description: "CLAP plugin"; Types: full custom
Name: "vst3"; Description: "VST3 plugin (where a host offers LV2 or CLAP, prefer them)"; Types: full custom
Name: "weidinger"; Description: "Weidinger, the brass (a preview): trumpet, horn, trombones, tuba, in the formats chosen above"; Types: full custom

[InstallDelete]
; what an earlier version installed: replaced whole
Type: filesandordirs; Name: "{commonappdata}\Dussek\instruments"
Type: filesandordirs; Name: "{commoncf64}\LV2\dussek.lv2"
Type: filesandordirs; Name: "{commoncf64}\VST3\dussek.vst3"
Type: files; Name: "{commoncf64}\CLAP\dussek.clap"
Type: files; Name: "{app}\dussek.exe"
Type: filesandordirs; Name: "{commoncf64}\LV2\weidinger.lv2"
Type: filesandordirs; Name: "{commoncf64}\VST3\weidinger.vst3"
Type: files; Name: "{commoncf64}\CLAP\weidinger.clap"
Type: files; Name: "{app}\weidinger.exe"
; the Start menu entries, gone with the standalones when an update leaves them out
Type: files; Name: "{autoprograms}\Dussek.lnk"
Type: files; Name: "{autoprograms}\Weidinger.lnk"

[Files]
Source: "{#Source}\dussek.exe"; DestDir: "{app}"; Components: standalone; Flags: ignoreversion
Source: "{#Source}\README.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Source}\LICENSE.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\icon\dussek.ico"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Source}\dussek.lv2\*"; DestDir: "{commoncf64}\LV2\dussek.lv2"; Components: lv2; Flags: ignoreversion recursesubdirs
Source: "{#Source}\dussek.clap"; DestDir: "{commoncf64}\CLAP"; Components: clap; Flags: ignoreversion
Source: "{#Source}\dussek.vst3\*"; DestDir: "{commoncf64}\VST3\dussek.vst3"; Components: vst3; Flags: ignoreversion recursesubdirs
Source: "{#Source}\instruments\*"; DestDir: "{commonappdata}\Dussek\instruments"; Flags: ignoreversion recursesubdirs
Source: "{#Source}\weidinger.exe"; DestDir: "{app}"; Components: weidinger and standalone; Flags: ignoreversion
Source: "{#Source}\weidinger.lv2\*"; DestDir: "{commoncf64}\LV2\weidinger.lv2"; Components: weidinger and lv2; Flags: ignoreversion recursesubdirs
Source: "{#Source}\weidinger.clap"; DestDir: "{commoncf64}\CLAP"; Components: weidinger and clap; Flags: ignoreversion
Source: "{#Source}\weidinger.vst3\*"; DestDir: "{commoncf64}\VST3\weidinger.vst3"; Components: weidinger and vst3; Flags: ignoreversion recursesubdirs
Source: "{#Source}\instruments-brass\*"; DestDir: "{commonappdata}\Dussek\instruments"; Components: weidinger; Flags: ignoreversion recursesubdirs

[Icons]
Name: "{autoprograms}\Dussek"; Filename: "{app}\dussek.exe"; IconFilename: "{app}\dussek.ico"; Components: standalone
Name: "{autoprograms}\Weidinger"; Filename: "{app}\weidinger.exe"; IconFilename: "{app}\dussek.ico"; Components: weidinger and standalone

[Run]
Filename: "{app}\dussek.exe"; Description: "Start Dussek"; Components: standalone; Flags: nowait postinstall skipifsilent

[UninstallDelete]
Type: filesandordirs; Name: "{commonappdata}\Dussek\instruments"
