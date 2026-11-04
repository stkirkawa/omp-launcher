Unicode true
RequestExecutionLevel user
SetCompressor /SOLID lzma
SetCompressorDictSize 32
Name "Kirkawa Launcher"
OutFile "..\release\KirkawaLauncherSetup.exe"
InstallDir "$LOCALAPPDATA\Programs\Kirkawa Launcher"
InstallDirRegKey HKCU "Software\KirkawaLauncher" "InstallDir"
BrandingText "KIRKAWA  |  SAN ANDREAS"

!include "MUI2.nsh"
!include "LogicLib.nsh"

!define MUI_ABORTWARNING
!define MUI_BGCOLOR "101113"
!define MUI_TEXTCOLOR "F4F1ED"
!define MUI_ICON "..\src-tauri\icons\icon.ico"
!define MUI_UNICON "..\src-tauri\icons\icon.ico"
!define MUI_WELCOMEPAGE_TITLE "Kirkawa Launcher"
!define MUI_WELCOMEPAGE_TEXT "Добро пожаловать в Сан-Андреас.$\r$\n$\r$\nУстановщик подготовит лаунчер Kirkawa и необходимые компоненты для запуска."
!define MUI_DIRECTORYPAGE_TEXT_TOP "Лаунчер будет установлен отдельно от GTA: San Andreas. Игровые файлы и пользовательские модификации не изменяются."
!define MUI_FINISHPAGE_RUN "$INSTDIR\KirkawaLauncher.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Запустить Kirkawa Launcher"
!define MUI_FINISHPAGE_RUN_NOTCHECKED
!define WEBVIEW2_CLIENT_GUID "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "Russian"

VIProductVersion "1.7.0.0"
VIAddVersionKey /LANG=${LANG_RUSSIAN} "ProductName" "Kirkawa Launcher"
VIAddVersionKey /LANG=${LANG_RUSSIAN} "FileDescription" "Установка Kirkawa Launcher"
VIAddVersionKey /LANG=${LANG_RUSSIAN} "ProductVersion" "1.7.0"
VIAddVersionKey /LANG=${LANG_RUSSIAN} "FileVersion" "1.7.0.0"
VIAddVersionKey /LANG=${LANG_RUSSIAN} "LegalCopyright" "Kirkawa"

Function IsWebView2RuntimeInstalled
  SetRegView 32
  ClearErrors
  ReadRegStr $1 HKLM "Software\Microsoft\EdgeUpdate\Clients\${WEBVIEW2_CLIENT_GUID}" "pv"
  ${If} $1 == ""
    ReadRegStr $1 HKCU "Software\Microsoft\EdgeUpdate\Clients\${WEBVIEW2_CLIENT_GUID}" "pv"
  ${EndIf}
  ${If} $1 == ""
    Push "0"
  ${Else}
    Push "1"
  ${EndIf}
FunctionEnd

Section "Kirkawa Launcher" MainSection
  SectionIn RO
  SetShellVarContext current

  Call IsWebView2RuntimeInstalled
  Pop $2
  ${If} $2 == "0"
    SetOutPath "$PLUGINSDIR"
    File "WebView2Bootstrapper.exe"
    DetailPrint "Проверяю Microsoft Edge WebView2 Runtime..."
    ExecWait '"$PLUGINSDIR\WebView2Bootstrapper.exe" /silent /install' $0
    ${If} $0 != 0
    ${AndIf} $0 != -2147219416
      MessageBox MB_ICONSTOP|MB_OK "Не удалось установить WebView2 Runtime (код $0). Подключитесь к интернету и повторите установку."
      Abort
    ${EndIf}

    Call IsWebView2RuntimeInstalled
    Pop $2
    ${If} $2 == "0"
      MessageBox MB_ICONSTOP|MB_OK "Установщик WebView2 завершился с кодом $0, но Runtime не найден. Перезагрузите компьютер и повторите установку."
      Abort
    ${EndIf}
  ${EndIf}

  SetOutPath "$INSTDIR"
  File "..\release\payload\KirkawaLauncher.exe"
  WriteUninstaller "$INSTDIR\Uninstall.exe"

  CreateDirectory "$SMPROGRAMS\Kirkawa Launcher"
  CreateShortcut "$SMPROGRAMS\Kirkawa Launcher\Kirkawa Launcher.lnk" "$INSTDIR\KirkawaLauncher.exe" "" "$INSTDIR\KirkawaLauncher.exe"

  WriteRegStr HKCU "Software\KirkawaLauncher" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\KirkawaLauncher" "DisplayName" "Kirkawa Launcher"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\KirkawaLauncher" "DisplayVersion" "1.7.0"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\KirkawaLauncher" "Publisher" "Kirkawa"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\KirkawaLauncher" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\KirkawaLauncher" "NoModify" 1
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\KirkawaLauncher" "NoRepair" 1
SectionEnd

Section /o "Создать ярлык на рабочем столе" DesktopShortcut
  CreateShortcut "$DESKTOP\Kirkawa Launcher.lnk" "$INSTDIR\KirkawaLauncher.exe" "" "$INSTDIR\KirkawaLauncher.exe"
SectionEnd

Section "Uninstall"
  SetShellVarContext current
  Delete "$DESKTOP\Kirkawa Launcher.lnk"
  Delete "$SMPROGRAMS\Kirkawa Launcher\Kirkawa Launcher.lnk"
  RMDir "$SMPROGRAMS\Kirkawa Launcher"

  Delete "$INSTDIR\KirkawaLauncher.exe"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"

  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\KirkawaLauncher"
  DeleteRegKey HKCU "Software\KirkawaLauncher"
SectionEnd
