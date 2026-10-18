#ifndef StageDir
  #error StageDir must point to the prepared portable directory
#endif
#ifndef OutDir
  #error OutDir must point to dist
#endif
[Setup]
AppId={{34FA3F08-63D0-4A64-9E61-F79E7078B39D}
AppName=HyperCast
AppVersion=0.8.14
AppPublisher=Lucas Camy
AppPublisherURL=https://github.com/LucasCamy/HyperCast
DefaultDirName={localappdata}\Programs\HyperCast
DefaultGroupName=HyperCast
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.19041
OutputDir={#OutDir}
OutputBaseFilename=HyperCast-0.8.14-windows-x64-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
UsePreviousLanguage=yes
UninstallDisplayIcon={app}\hypercast.exe
SetupIconFile=..\assets\hypercast.ico
LicenseFile=..\LICENSE
CloseApplications=yes
[Languages]
Name: "brazilianportuguese"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"
[CustomMessages]
brazilianportuguese.DesktopShortcut=Criar atalho na área de trabalho
english.DesktopShortcut=Create a desktop shortcut
brazilianportuguese.ShortcutGroup=Atalhos:
english.ShortcutGroup=Shortcuts:
brazilianportuguese.LaunchHyperCast=Abrir HyperCast
english.LaunchHyperCast=Launch HyperCast
[Tasks]
Name: "desktopicon"; Description: "{cm:DesktopShortcut}"; GroupDescription: "{cm:ShortcutGroup}"; Flags: unchecked
[Files]
Source: "{#StageDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs
[Icons]
Name: "{group}\HyperCast"; Filename: "{app}\hypercast.exe"; Parameters: "--gui"
Name: "{autodesktop}\HyperCast"; Filename: "{app}\hypercast.exe"; Parameters: "--gui"; Tasks: desktopicon
[Run]
Filename: "{app}\hypercast.exe"; Parameters: "--gui"; Description: "{cm:LaunchHyperCast}"; Flags: nowait postinstall skipifsilent
Filename: "{app}\hypercast.exe"; Parameters: "--gui"; Flags: nowait runasoriginaluser; Check: IsAutomaticUpdate

[Code]
function IsAutomaticUpdate(): Boolean;
begin
  Result := WizardSilent and (ExpandConstant('{param:HYPERCASTUPDATE|0}') = '1');
end;

procedure CurStepChanged(CurStep: TSetupStep);
var
  LanguagePath: String;
  LanguageCode: String;
begin
  if CurStep = ssPostInstall then
  begin
    LanguagePath := ExpandConstant('{localappdata}\HyperCast\language');
    { Silent updates preserve the application's existing language. }
    if WizardSilent and FileExists(LanguagePath) then Exit;
    if ActiveLanguage = 'english' then LanguageCode := 'en-US' else LanguageCode := 'pt-BR';
    if ForceDirectories(ExpandConstant('{localappdata}\HyperCast')) then
      SaveStringToFile(LanguagePath, LanguageCode, False);
  end;
end;
