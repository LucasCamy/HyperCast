param([Parameter(Mandatory=$true)][string]$NotesFile)
$ErrorActionPreference = 'Stop'
$hcRoot = Split-Path -Parent $PSScriptRoot
$hcVersion = (Select-String -LiteralPath "$hcRoot/Cargo.toml" -Pattern '^version = "([0-9]+\.[0-9]+\.[0-9]+)"$' | Select-Object -First 1).Matches.Groups[1].Value
if (-not $hcVersion) { throw 'Stable package version not found' }
$hcRepo = 'LucasCamy/HyperCast'
$hcTag = "v$hcVersion"
$hcFiles = @("$hcRoot/dist/HyperCast-$hcVersion-windows-x64-setup.exe", "$hcRoot/dist/HyperCast-$hcVersion-windows-x64-portable.zip", "$hcRoot/dist/SHA256SUMS.txt")
foreach ($hcFile in $hcFiles) { if (-not (Test-Path -LiteralPath $hcFile -PathType Leaf)) { throw "Package missing: $hcFile" } }
$hcHashes = Get-Content -LiteralPath $hcFiles[2]
foreach ($hcFile in $hcFiles[0..1]) {
    $hcExpected = (Get-FileHash -LiteralPath $hcFile -Algorithm SHA256).Hash.ToLower() + '  ' + (Split-Path -Leaf $hcFile)
    if ($hcExpected -notin $hcHashes) { throw 'Package checksum mismatch' }
}
if (-not (Test-Path -LiteralPath $NotesFile -PathType Leaf)) { throw 'Release notes missing' }
$hcDirty = & git -C $hcRoot status --porcelain
if ($LASTEXITCODE -ne 0 -or $hcDirty) { throw 'Commit and push the complete source before publishing' }
$hcCommit = & git -C $hcRoot rev-parse HEAD
if ($LASTEXITCODE -ne 0) { throw 'Cannot read source commit' }
# Create all assets as a draft first: /releases/latest must never see an incomplete release.
& gh release create $hcTag @hcFiles --repo $hcRepo --target $hcCommit --draft --title "HyperCast $hcVersion" --notes-file $NotesFile
if ($LASTEXITCODE -ne 0) { throw 'Draft creation failed; inspect existing release before retrying' }
$hcRelease = & gh release view $hcTag --repo $hcRepo --json assets | ConvertFrom-Json
foreach ($hcFile in $hcFiles) {
    if ((Split-Path -Leaf $hcFile) -notin $hcRelease.assets.name) { throw 'Draft assets incomplete; not publishing' }
}
& gh release edit $hcTag --repo $hcRepo --draft=false --latest
if ($LASTEXITCODE -ne 0) { throw 'Release publication failed' }
Write-Output "Published https://github.com/$hcRepo/releases/tag/$hcTag"
