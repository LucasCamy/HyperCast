$ErrorActionPreference='Stop'
$hcRoot=Split-Path -Parent $PSScriptRoot
$hcVersion=(Select-String -LiteralPath "$hcRoot/Cargo.toml" -Pattern '^version = "([0-9.]+)"$' | Select-Object -First 1).Matches.Groups[1].Value
$hcRegistry='HKCU:/Software/Microsoft/Windows/CurrentVersion/Uninstall/{34FA3F08-63D0-4A64-9E61-F79E7078B39D}_is1'
if(Test-Path $hcRegistry){throw 'Existing installation found; refusing to overwrite it'}
if(Get-Process hypercast -ErrorAction SilentlyContinue){throw 'An application is running; refusing to touch it'}
$hcTest=[IO.Path]::GetFullPath((Join-Path $hcRoot '.tools/install-language-test'))
if(-not $hcTest.StartsWith([IO.Path]::GetFullPath($hcRoot)+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)){throw 'Test folder escaped workspace'}
$hcLanguage=Join-Path $env:LOCALAPPDATA 'HyperCast/language'
$hcHadLanguage=Test-Path -LiteralPath $hcLanguage
$hcSavedLanguage=if($hcHadLanguage){[IO.File]::ReadAllBytes($hcLanguage)}else{$null}
$hcPreviousExe=$env:HYPERCAST_TEST_EXE
$hcInstaller="$hcRoot/dist/HyperCast-$hcVersion-windows-x64-setup.exe"
try {
    # Remove only this non-secret preference for the isolated first-install case.
    if($hcHadLanguage){Remove-Item -LiteralPath $hcLanguage}
    $hcInstall=Start-Process -FilePath $hcInstaller -ArgumentList @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART','/LANG=english',"/DIR=`"$hcTest`"") -WindowStyle Hidden -Wait -PassThru
    if($hcInstall.ExitCode -ne 0){throw 'English test installation failed'}
    if([IO.File]::ReadAllText($hcLanguage).Trim() -ne 'en-US'){throw 'Installer did not persist English'}
    $env:HYPERCAST_TEST_EXE="$hcTest/hypercast.exe"
    python -m tests.integration_language_ui
    if($LASTEXITCODE -ne 0){throw 'Installed English GUI validation failed'}
    $hcInstall=Start-Process -FilePath $hcInstaller -ArgumentList @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART','/LANG=brazilianportuguese',"/DIR=`"$hcTest`"") -WindowStyle Hidden -Wait -PassThru
    if($hcInstall.ExitCode -ne 0){throw 'Silent update test failed'}
    if([IO.File]::ReadAllText($hcLanguage).Trim() -ne 'en-US'){throw 'Silent update overwrote the saved language'}
    Write-Output 'PASS: English installer preference and silent-update preservation'
} finally {
    $env:HYPERCAST_TEST_EXE=$hcPreviousExe
    $hcUninstallFailed=$false
    $hcUninstaller=[IO.Path]::GetFullPath((Join-Path $hcTest 'unins000.exe'))
    if($hcUninstaller.StartsWith($hcTest+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase) -and (Test-Path -LiteralPath $hcUninstaller)){
        $hcUninstall=Start-Process -FilePath $hcUninstaller -ArgumentList @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART') -WindowStyle Hidden -Wait -PassThru
        if($hcUninstall.ExitCode -ne 0){$hcUninstallFailed=$true}
    }
    if($hcHadLanguage){[IO.File]::WriteAllBytes($hcLanguage,$hcSavedLanguage)}elseif(Test-Path -LiteralPath $hcLanguage){Remove-Item -LiteralPath $hcLanguage}
    if($hcUninstallFailed){throw 'Test uninstall failed'}
}
