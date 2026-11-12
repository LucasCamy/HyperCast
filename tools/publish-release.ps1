param([Parameter(Mandatory=$true)][string]$NotesFile, [switch]$RequireLinux)
$ErrorActionPreference = 'Stop'
$hcRoot = Split-Path -Parent $PSScriptRoot
$hcVersion = (Select-String -LiteralPath "$hcRoot/Cargo.toml" -Pattern '^version = "([0-9]+\.[0-9]+\.[0-9]+)"$' | Select-Object -First 1).Matches.Groups[1].Value
if (-not $hcVersion) { throw 'Stable package version not found' }
$hcRepo = 'LucasCamy/HyperCast'
$hcTag = "v$hcVersion"
$hcFiles = @("$hcRoot/dist/HyperCast-$hcVersion-windows-x64-setup.exe", "$hcRoot/dist/HyperCast-$hcVersion-windows-x64-portable.zip", "$hcRoot/dist/SHA256SUMS.txt")
$hcLinuxFile="$hcRoot/dist/HyperCast-$hcVersion-linux-x86_64.AppImage"
$hcLinuxHashes="$hcRoot/dist/SHA256SUMS-linux.txt"
if ($RequireLinux -and -not (Test-Path -LiteralPath $hcLinuxFile -PathType Leaf)) {
    throw 'Linux AppImage missing'
}
if(Test-Path -LiteralPath $hcLinuxFile -PathType Leaf){
    if(-not(Test-Path -LiteralPath $hcLinuxHashes -PathType Leaf)){throw 'Linux package checksum missing'}
    $hcLinuxExpected=(Get-FileHash -LiteralPath $hcLinuxFile -Algorithm SHA256).Hash.ToLower()+'  '+(Split-Path -Leaf $hcLinuxFile)
    if($hcLinuxExpected -notin (Get-Content -LiteralPath $hcLinuxHashes)){throw 'Linux package checksum mismatch'}
    $hcFiles+=@($hcLinuxFile,$hcLinuxHashes)
    $hcLinuxSource="$hcRoot/dist/HyperCast-$hcVersion-linux-corresponding-source.tar.gz"
    if(-not(Test-Path -LiteralPath $hcLinuxSource -PathType Leaf)){throw 'Linux corresponding-source archive missing'}
    $hcSourceExpected=(Get-FileHash -LiteralPath $hcLinuxSource -Algorithm SHA256).Hash.ToLower()+'  '+(Split-Path -Leaf $hcLinuxSource)
    if($hcSourceExpected -notin (Get-Content -LiteralPath $hcLinuxHashes)){throw 'Linux source checksum mismatch'}
    $hcFiles+=$hcLinuxSource
    $hcMediaSource="$hcRoot/dist/HyperCast-$hcVersion-linux-media-source.tar.gz"
    if(-not(Test-Path -LiteralPath $hcMediaSource -PathType Leaf)){throw 'Pinned Linux media source missing'}
    $hcMediaExpected=(Get-FileHash -LiteralPath $hcMediaSource -Algorithm SHA256).Hash.ToLower()+'  '+(Split-Path -Leaf $hcMediaSource)
    if($hcMediaExpected -notin (Get-Content -LiteralPath $hcLinuxHashes)){throw 'Media source checksum mismatch'}
    $hcFiles+=$hcMediaSource
}
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
$hcTagRefs = & git -C $hcRoot ls-remote origin "refs/tags/$hcTag" "refs/tags/$hcTag^{}"
if ($LASTEXITCODE -ne 0) { throw 'Cannot verify the remote tag' }
if ($hcTagRefs -and (($hcTagRefs | Select-Object -Last 1) -split '\s+')[0] -ne $hcCommit) {
    throw "Tag $hcTag already points to a different commit"
}

function Get-ReleaseDraft {
    $releaseJson = & gh release view $hcTag --repo $hcRepo --json isDraft,targetCommitish,assets
    if ($LASTEXITCODE -ne 0) { throw 'Cannot read the release draft' }
    $release = $releaseJson | ConvertFrom-Json
    if (-not $release.isDraft -or $release.targetCommitish -ne $hcCommit) {
        throw 'Expected a release draft for the tested commit'
    }
    $release
}

# Stage and verify every file before exposing the release to the in-app updater.
& gh release view $hcTag --repo $hcRepo --json isDraft 2>$null | Out-Null
if ($LASTEXITCODE -ne 0) {
    & gh release create $hcTag --repo $hcRepo --target $hcCommit --draft --title "HyperCast $hcVersion" --notes-file $NotesFile
    if ($LASTEXITCODE -ne 0) { throw 'Draft creation failed' }
}
$hcRelease = Get-ReleaseDraft
$hcExpectedAssets = @{}
foreach ($hcFile in $hcFiles) {
    $file = Get-Item -LiteralPath $hcFile
    $hcExpectedAssets[$file.Name] = @{
        Path = $file.FullName
        Size = $file.Length
        Digest = 'sha256:' + (Get-FileHash -LiteralPath $hcFile -Algorithm SHA256).Hash.ToLowerInvariant()
    }
}
foreach ($asset in $hcRelease.assets) {
    if (-not $hcExpectedAssets.ContainsKey($asset.name)) {
        throw "Unexpected draft asset $($asset.name)"
    }
}
function Test-ReleaseAsset($asset, $expected) {
    $null -ne $asset -and $asset.state -eq 'uploaded' -and
        $asset.size -eq $expected.Size -and $asset.digest -eq $expected.Digest
}
foreach ($name in $hcExpectedAssets.Keys) {
    $asset = $hcRelease.assets | Where-Object name -CEQ $name
    $expected = $hcExpectedAssets[$name]
    if (-not (Test-ReleaseAsset $asset $expected)) {
        & gh release upload $hcTag $expected.Path --repo $hcRepo --clobber
        if ($LASTEXITCODE -ne 0) { throw "Upload failed for $name" }
    }
}
$hcRelease = Get-ReleaseDraft
if ($hcRelease.assets.Count -ne $hcExpectedAssets.Count) { throw 'Draft asset count mismatch' }
foreach ($asset in $hcRelease.assets) {
    if (-not (Test-ReleaseAsset $asset $hcExpectedAssets[$asset.name])) {
        throw "Draft integrity check failed for $($asset.name)"
    }
}
& gh release edit $hcTag --repo $hcRepo --draft=false --latest
if ($LASTEXITCODE -ne 0) { throw 'Release publication failed' }
Write-Output "Published https://github.com/$hcRepo/releases/tag/$hcTag"
