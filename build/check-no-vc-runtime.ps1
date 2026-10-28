# B-56: no shipped Windows binary needs the Visual C++ redistributable. Lists every .exe and .dll under the given
# files and folders (folders recursively) with `dumpbin /dependents` (load-time and delay-load imports) and fails if
# any imports a VC++ runtime DLL (msvcp140*, vcruntime140*, concrt140, vccorlib140, vcomp140*), or if a file named
# with -Require was not among them.
#   pwsh build/check-no-vc-runtime.ps1 <file or folder>... [-Require borealis.exe,llama.dll,...]
param(
    [Parameter(Mandatory = $true, Position = 0, ValueFromRemainingArguments = $true)][string[]]$Path,
    [string[]]$Require = @()
)
$ErrorActionPreference = 'Stop'

$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
$dumpbin = & $vswhere -latest -products * -find 'VC\Tools\MSVC\**\bin\Hostx64\x64\dumpbin.exe' | Select-Object -First 1
if (-not $dumpbin) { Write-Output '::error::dumpbin.exe not found (Visual Studio C++ tools)'; exit 1 }

$vcRuntime = '^(msvcp\d+(_\w+)?|vcruntime\d+(_\d+)?|concrt\d+|vccorlib\d+|vcomp\d+d?)\.dll$'
$files = foreach ($p in $Path) {
    if (-not (Test-Path $p)) { Write-Output "::error::$p does not exist"; exit 1 }
    $item = Get-Item $p
    if ($item.PSIsContainer) {
        Get-ChildItem -Path $item.FullName -Recurse -File | Where-Object { $_.Extension -in '.exe', '.dll' }
    } else {
        $item
    }
}
$files = @($files | Sort-Object FullName -Unique)
if ($files.Count -eq 0) { Write-Output "::error::no .exe or .dll under $($Path -join ', ')"; exit 1 }

$bad = 0
foreach ($f in $files) {
    $out = & $dumpbin /nologo /dependents $f.FullName
    if ($LASTEXITCODE -ne 0) { Write-Output "::error::dumpbin failed on $($f.FullName)"; exit 1 }
    $imports = @($out | ForEach-Object { if ($_ -match '^\s+(\S+\.dll)\s*$') { $Matches[1] } } | Sort-Object -Unique)
    $vc = @($imports | Where-Object { $_ -match $vcRuntime })
    if ($vc.Count -gt 0) {
        Write-Output "::error::$($f.FullName) imports the VC++ runtime: $($vc -join ', ')"
        $bad++
    } else {
        $crt = @($imports | Where-Object { $_ -match '^(api-ms-win-crt-|ucrtbase)' }).Count
        Write-Output ("ok  {0}: {1} imports ({2} UCRT), no VC++ runtime" -f $f.FullName, $imports.Count, $crt)
    }
}

$missing = @($Require | Where-Object { $name = $_; -not ($files | Where-Object Name -eq $name) })
foreach ($m in $missing) { Write-Output "::error::$m was not among the checked files" }

Write-Output "$($files.Count) files checked, $bad import the VC++ runtime"
if ($bad -gt 0 -or $missing.Count -gt 0) { exit 1 }
exit 0
