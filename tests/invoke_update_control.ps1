param([int]$TargetPid,[string]$ExpectedExecutable,[string]$Name)
$ErrorActionPreference='Stop'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$hcProcess=Get-Process -Id $TargetPid
if ([IO.Path]::GetFullPath($hcProcess.Path) -ne [IO.Path]::GetFullPath($ExpectedExecutable)) { throw 'Refusing UI access outside the owned test executable' }
if (-not $hcProcess.MainWindowHandle) { throw 'Test window is not ready' }
$hcRoot=[System.Windows.Automation.AutomationElement]::FromHandle($hcProcess.MainWindowHandle)
$hcCondition=New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty,$Name)
$hcControl=$hcRoot.FindFirst([System.Windows.Automation.TreeScope]::Descendants,$hcCondition)
if (-not $hcControl) { throw "Test control unavailable: $Name" }
$hcAction=$hcControl.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
$hcAction.Invoke()
Write-Output "Invoked owned control: $Name"
