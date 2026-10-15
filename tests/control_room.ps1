param([int]$TargetPid,[string]$ExpectedExecutable,[string]$Name,[switch]$Double,[switch]$Read,[switch]$List,[string]$FocusSequence,[switch]$Toggle)
$ErrorActionPreference='Stop'
[Console]::OutputEncoding=New-Object System.Text.UTF8Encoding($false)
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$hcProcess=Get-Process -Id $TargetPid
if([IO.Path]::GetFullPath($hcProcess.Path) -ne [IO.Path]::GetFullPath($ExpectedExecutable)){throw 'Unexpected executable'}
$hcRoot=[System.Windows.Automation.AutomationElement]::FromHandle($hcProcess.MainWindowHandle)
if($List){
$hcRoot.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition) | ForEach-Object {if($_.Current.Name -like 'Transmissão de *'){$_.Current.Name}};exit
}
if($FocusSequence){
    foreach($hcLabel in $FocusSequence.Split(',')){
        $hcNodes=$hcRoot.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition)
        $hcFocus=$hcNodes | Where-Object {$_.Current.Name.StartsWith($hcLabel+': ')} | Select-Object -First 1
        if(-not $hcFocus){throw 'Owned participant control unavailable'}
        $hcFocus.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
        Start-Sleep -Milliseconds 60
    };exit
}
$hcCondition=New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty,$Name)
$hcNode=$hcRoot.FindFirst([System.Windows.Automation.TreeScope]::Descendants,$hcCondition)
if(-not $hcNode){throw "Owned control missing: $Name"}
if($Toggle){$hcNode.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle();exit}
if($Read){$hcNode.Current.BoundingRectangle | ConvertTo-Json -Compress; exit}
if($Double){
Add-Type -TypeDefinition @"
using System; using System.Runtime.InteropServices;
public class HcClick {
[DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr w);
[DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
[DllImport("user32.dll")] public static extern bool SetCursorPos(int x,int y);
[DllImport("user32.dll")] public static extern bool GetCursorPos(out Point p);
[DllImport("user32.dll")] public static extern void mouse_event(uint f,uint x,uint y,uint d,UIntPtr e);
[StructLayout(LayoutKind.Sequential)] public struct Point {public int x,y;}
}
"@
$hcRect=$hcNode.Current.BoundingRectangle
if($hcRect.IsEmpty -or $hcNode.Current.IsOffscreen){throw 'Owned control is off screen'}
[HcClick]::SetForegroundWindow($hcProcess.MainWindowHandle)|Out-Null
Start-Sleep -Milliseconds 200
if([HcClick]::GetForegroundWindow() -ne $hcProcess.MainWindowHandle){throw 'Owned window did not receive focus'}
$hcOld=New-Object HcClick+Point
[HcClick]::GetCursorPos([ref]$hcOld)|Out-Null
try {
[HcClick]::SetCursorPos([int]($hcRect.X+$hcRect.Width/2),[int]($hcRect.Y+$hcRect.Height/2))|Out-Null
Start-Sleep -Milliseconds 150
foreach($hcClick in 1..2){
[HcClick]::mouse_event(2,0,0,0,[UIntPtr]::Zero)
Start-Sleep -Milliseconds 40
[HcClick]::mouse_event(4,0,0,0,[UIntPtr]::Zero)
Start-Sleep -Milliseconds 70
}
}finally{[HcClick]::SetCursorPos($hcOld.x,$hcOld.y)|Out-Null}
}else{
$hcAction=$null
if($hcNode.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern,[ref]$hcAction)){$hcAction.Invoke()}
else{$hcNode.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle()}
}
