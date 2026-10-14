param([int]$TargetPid,[string]$ExpectedExecutable,[ValidateSet('room','login')][string]$Mode='room', [string]$InputFile)
$ErrorActionPreference='Stop'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public static class HCRoomInput {
 [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);
 public static void TypeText(IntPtr window, string text) {
  foreach(char ch in text) {
   PostMessageW(window,0x100,(IntPtr)0x87,(IntPtr)1);
   PostMessageW(window,0x102,(IntPtr)(int)ch,(IntPtr)1);
   PostMessageW(window,0x101,(IntPtr)0x87,(IntPtr)0xC0000001L);
  }
 }
}
"@
$hcProcess=Get-Process -Id $TargetPid
if ([IO.Path]::GetFullPath($hcProcess.Path) -ne [IO.Path]::GetFullPath($ExpectedExecutable)) { throw 'Unexpected test executable' }
$hcRoot=[System.Windows.Automation.AutomationElement]::FromHandle($hcProcess.MainWindowHandle)
$hcInput=Get-Content -LiteralPath $InputFile -Raw -Encoding UTF8 | ConvertFrom-Json
$hcFields=if ($Mode -eq 'login') { @('Chave de acesso') } else { @('Nome da sala','Seu nome') }
foreach ($hcName in $hcFields) {
    $hcCondition=New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty,$hcName)
    $hcControl=$hcRoot.FindAll([System.Windows.Automation.TreeScope]::Descendants,$hcCondition) | Where-Object { $_.Current.ControlType -eq [System.Windows.Automation.ControlType]::Edit } | Select-Object -First 1
    if (-not $hcControl) { throw 'Owned creation field unavailable' }
    $hcValue=switch ($hcName) { 'Nome da sala' { $hcInput.title } 'Seu nome' { $hcInput.name } 'Chave de acesso' { $hcInput.code } }
    $hcControl.SetFocus()
    Start-Sleep -Milliseconds 650
    [HCRoomInput]::TypeText($hcProcess.MainWindowHandle,$hcValue)
    Start-Sleep -Milliseconds 500
    if ($hcName -eq 'Chave de acesso') {
        $hcPattern=$hcControl.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern)
        if ($hcPattern.Current.Value.Length -ne $hcValue.Length) { throw 'Owned password length did not match input' }
    } else {
        $hcPattern=$hcControl.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern)
        if ($hcPattern.Current.Value -ne $hcValue) { throw 'Owned field did not receive exact test input' }
    }
}
Write-Output 'Filled owned creation form (values not logged)'
