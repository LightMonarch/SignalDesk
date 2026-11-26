param(
    [string]$CacheDirectory = (Join-Path $env:TEMP 'silicon-desk-sensors-build')
)
$ErrorActionPreference = 'Stop'
$version = '0.9.6'
$sha256 = '086d9f1b5a99e643edc2cfaaac16051685b551e4c5ac0b32a57c58c0e529c001'
$cache = (New-Item -ItemType Directory -Force -Path $CacheDirectory).FullName
$archive = Join-Path $cache "LibreHardwareMonitor-v$version.zip"
if (!(Test-Path -LiteralPath $archive) -or (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ine $sha256) {
    $download = Join-Path $cache ("LibreHardwareMonitor-" + [Guid]::NewGuid() + ".download")
    try {
        curl.exe --fail --location --retry 2 --connect-timeout 15 --max-time 300 "https://github.com/LibreHardwareMonitor/LibreHardwareMonitor/releases/download/v$version/LibreHardwareMonitor.zip" --output $download
        if ($LASTEXITCODE -ne 0) { throw 'LibreHardwareMonitor download failed.' }
        if ((Get-FileHash -LiteralPath $download -Algorithm SHA256).Hash -ine $sha256) {
            throw 'LibreHardwareMonitor archive checksum mismatch.'
        }
        Move-Item -LiteralPath $download -Destination $archive -Force
    } finally {
        Remove-Item -LiteralPath $download -ErrorAction SilentlyContinue
    }
}
$vendor = (New-Item -ItemType Directory -Force -Path (Join-Path $cache 'vendor')).FullName
tar.exe -xf $archive -C $vendor
if ($LASTEXITCODE -ne 0) { throw 'Could not extract the verified LibreHardwareMonitor archive.' }

$library = Get-ChildItem -LiteralPath $vendor -Recurse -Filter LibreHardwareMonitorLib.dll -File | Select-Object -First 1
if (!$library) { throw 'LibreHardwareMonitorLib.dll is missing.' }
$project = Split-Path -Parent $PSScriptRoot
$runtime = (New-Item -ItemType Directory -Force -Path (Join-Path $project 'src-tauri\sensor-runtime')).FullName
foreach ($file in Get-ChildItem -LiteralPath $library.DirectoryName -Filter '*.dll' -File) {
    if ($file.Name -notmatch '^(Aga\.|OxyPlot|Translucent|MaterialSkin|UIAutomation)') {
        Copy-Item -LiteralPath $file.FullName -Destination $runtime -Force
    }
}
$license = Join-Path $runtime 'NOTICE.txt'
@"
Silicon Desk Thermal Reader uses LibreHardwareMonitorLib $version.
The unmodified upstream library and runtime dependencies were taken from the official v$version distribution.
LibreHardwareMonitorLib is distributed under the Mozilla Public License 2.0.
Source and license https://github.com/LibreHardwareMonitor/LibreHardwareMonitor/tree/v$version
Official distribution https://github.com/LibreHardwareMonitor/LibreHardwareMonitor/releases/tag/v$version
Other runtime DLLs retain their upstream licenses.
The PawnIO kernel driver is a separate installation and is not embedded in this helper.
"@ | Set-Content -LiteralPath $license -Encoding ascii
Get-ChildItem -LiteralPath $library.DirectoryName -File |
    Where-Object Name -Match '^(LICENSE|COPYING|THIRD.PARTY)' |
    ForEach-Object { Copy-Item -LiteralPath $_.FullName -Destination $runtime -Force }

$framework = Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319'
$csc = Join-Path $framework 'csc.exe'
if (!(Test-Path -LiteralPath $csc)) { throw 'The Windows .NET Framework C# compiler is unavailable.' }
$helper = Join-Path $runtime 'ThermalReader.exe'
$binding = Join-Path $library.DirectoryName 'LibreHardwareMonitor.exe.config'
if (Test-Path -LiteralPath $binding) {
    Copy-Item -LiteralPath $binding -Destination "$helper.config" -Force
}
& $csc /nologo /target:winexe /platform:x64 /optimize+ "/out:$helper" "/reference:$($library.FullName)" "/reference:$(Join-Path $framework 'System.Web.Extensions.dll')" (Join-Path $PSScriptRoot 'ThermalReader.cs')
if ($LASTEXITCODE -ne 0) { throw 'ThermalReader compilation failed.' }
$p = Start-Process $helper -ArgumentList '--check' -Wait -PassThru -WindowStyle Hidden
if ($p.ExitCode -ne 0) {
    throw 'The helper failed to load the LibreHardwareMonitor assembly.'
}
"Thermal sensor helper staged with LibreHardwareMonitorLib $version."
