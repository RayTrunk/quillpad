; Installer für Quillpad (Inno Setup 6).
; Bauen:  cargo build --release
;         ISCC packaging\windows\quillpad.iss
; Die Version wird aus der fertigen exe gelesen; der Installer landet in target\installer.

#define AppExe "..\..\target\release\quillpad.exe"
#define AppVersion GetStringFileInfo(AppExe, "ProductVersion")
#if AppVersion == ""
  #error Version nicht lesbar - zuerst "cargo build --release" ausführen.
#endif

; Dateiendungen für „Öffnen mit“ und die Standard-Apps-Einstellungen.
#dim Exts[60] {"txt", "text", "md", "markdown", "log", "ini", "cfg", "conf", "properties", "env", "json", "jsonc", "xml", "yaml", "yml", "toml", "csv", "tsv", "rs", "py", "pyw", "js", "mjs", "ts", "jsx", "tsx", "c", "h", "cpp", "hpp", "cc", "cs", "java", "kt", "go", "rb", "php", "lua", "pl", "swift", "sql", "sh", "bash", "ps1", "psm1", "psd1", "bat", "cmd", "vbs", "html", "htm", "css", "scss", "less", "svg", "gitignore", "gitattributes", "editorconfig", "diff", "patch"}
#define I

[Setup]
AppId={{7C3E5A12-4B8D-4F2A-9E61-2D5C8B7A9F30}
AppName=Quillpad
AppVersion={#AppVersion}
AppVerName=Quillpad {#AppVersion}
AppPublisher=R. Trunk
AppCopyright=© 2026 R. Trunk
AppPublisherURL=https://github.com/RayTrunk/quillpad
AppSupportURL=https://github.com/RayTrunk/quillpad/issues
AppUpdatesURL=https://github.com/RayTrunk/quillpad/releases
VersionInfoVersion={#AppVersion}
DefaultDirName={autopf}\Quillpad
DisableProgramGroupPage=yes
; Standard: nur für den aktuellen Benutzer, ohne Administratorrechte. Im Dialog wählbar: alle Benutzer.
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
OutputDir=..\..\target\installer
OutputBaseFilename=quillpad-{#AppVersion}-windows-x64-setup
SetupIconFile=..\..\assets\quillpad.ico
; Lizenz wird im Assistenten angezeigt (GNU GPL v3).
LicenseFile=..\..\LICENSE
UninstallDisplayIcon={app}\quillpad.exe
UninstallDisplayName=Quillpad
WizardStyle=modern
Compression=lzma2/max
SolidCompression=yes
ChangesAssociations=yes
; Ein laufendes Quillpad wird vor dem Aktualisieren geschlossen.
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "de"; MessagesFile: "compiler:Languages\German.isl"

[Tasks]
Name: "openwith"; Description: "Unter „Öffnen mit“ für Textdateien anbieten"; GroupDescription: "Einbindung in Windows:"
Name: "contextmenu"; Description: "Eintrag „Mit Quillpad bearbeiten“ im Kontextmenü aller Dateien"; GroupDescription: "Einbindung in Windows:"
Name: "defaulttxt"; Description: "Quillpad als Standard für .txt wählen (Windows fragt danach selbst nach)"; GroupDescription: "Einbindung in Windows:"; Flags: unchecked
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Quillpad"; Filename: "{app}\quillpad.exe"; Comment: "Schneller Texteditor"
Name: "{autodesktop}\Quillpad"; Filename: "{app}\quillpad.exe"; Tasks: desktopicon

[Registry]
; Programm bei Windows bekannt machen
Root: HKA; Subkey: "Software\Classes\Applications\quillpad.exe"; ValueType: string; ValueName: "FriendlyAppName"; ValueData: "Quillpad"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Applications\quillpad.exe\DefaultIcon"; ValueType: string; ValueData: "{app}\quillpad.exe,0"
Root: HKA; Subkey: "Software\Classes\Applications\quillpad.exe\shell\open\command"; ValueType: string; ValueData: """{app}\quillpad.exe"" ""%1"""
; Dateityp, unter dem Quillpad Textdateien öffnet
Root: HKA; Subkey: "Software\Classes\Quillpad.Textdatei"; ValueType: string; ValueData: "Textdatei (Quillpad)"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Quillpad.Textdatei\DefaultIcon"; ValueType: string; ValueData: "{app}\quillpad.exe,0"
Root: HKA; Subkey: "Software\Classes\Quillpad.Textdatei\shell\open\command"; ValueType: string; ValueData: """{app}\quillpad.exe"" ""%1"""
; Fähigkeiten für die Einstellungen „Standard-Apps“
Root: HKA; Subkey: "Software\Quillpad"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Quillpad\Capabilities"; ValueType: string; ValueName: "ApplicationName"; ValueData: "Quillpad"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Quillpad\Capabilities"; ValueType: string; ValueName: "ApplicationDescription"; ValueData: "Schneller Texteditor mit Tabs, Syntaxfarben und Suchleiste"
Root: HKA; Subkey: "Software\Quillpad\Capabilities"; ValueType: string; ValueName: "ApplicationIcon"; ValueData: "{app}\quillpad.exe,0"
Root: HKA; Subkey: "Software\RegisteredApplications"; ValueType: string; ValueName: "Quillpad"; ValueData: "Software\Quillpad\Capabilities"; Flags: uninsdeletevalue
; Kontextmenü „Mit Quillpad bearbeiten“
Root: HKA; Subkey: "Software\Classes\*\shell\Quillpad"; ValueType: string; ValueData: "Mit Quillpad bearbeiten"; Flags: uninsdeletekey; Tasks: contextmenu
Root: HKA; Subkey: "Software\Classes\*\shell\Quillpad"; ValueType: string; ValueName: "Icon"; ValueData: "{app}\quillpad.exe,0"; Tasks: contextmenu
Root: HKA; Subkey: "Software\Classes\*\shell\Quillpad\command"; ValueType: string; ValueData: """{app}\quillpad.exe"" ""%1"""; Tasks: contextmenu
; Je Dateiendung: „Öffnen mit“ und Eintrag in den Fähigkeiten
#sub EmitExt
Root: HKA; Subkey: "Software\Classes\.{#Exts[I]}\OpenWithProgids"; ValueType: string; ValueName: "Quillpad.Textdatei"; ValueData: ""; Flags: uninsdeletevalue; Tasks: openwith
Root: HKA; Subkey: "Software\Classes\Applications\quillpad.exe\SupportedTypes"; ValueType: string; ValueName: ".{#Exts[I]}"; ValueData: ""; Tasks: openwith
Root: HKA; Subkey: "Software\Quillpad\Capabilities\FileAssociations"; ValueType: string; ValueName: ".{#Exts[I]}"; ValueData: "Quillpad.Textdatei"
#endsub
#for {I = 0; I < DimOf(Exts); I++} EmitExt

[Run]
; Windows erlaubt Programmen nicht, sich selbst zum Standard zu machen: die Einstellungen öffnen.
Filename: "ms-settings:defaultapps?registeredAppUser=Quillpad"; Description: "Standard-Apps öffnen, um Quillpad für .txt zu wählen"; Flags: shellexec postinstall skipifsilent; Tasks: defaulttxt; Check: not IsAdminInstallMode
Filename: "ms-settings:defaultapps?registeredAppMachine=Quillpad"; Description: "Standard-Apps öffnen, um Quillpad für .txt zu wählen"; Flags: shellexec postinstall skipifsilent; Tasks: defaulttxt; Check: IsAdminInstallMode
Filename: "{app}\quillpad.exe"; Description: "{cm:LaunchProgram,Quillpad}"; Flags: nowait postinstall skipifsilent

[Code]
// Beim Deinstallieren fragen, ob auch Einstellungen, Sitzung und Sicherungen weg sollen.
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Dir: String;
begin
  if CurUninstallStep = usPostUninstall then
  begin
    Dir := ExpandConstant('{userappdata}\Quillpad');
    if DirExists(Dir) and not UninstallSilent then
      if MsgBox('Sollen auch die Einstellungen, die Sitzung und die Sicherungskopien von Quillpad gelöscht werden?' + #13#10#13#10 + Dir,
                mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES then
        DelTree(Dir, True, True, True);
  end;
end;
