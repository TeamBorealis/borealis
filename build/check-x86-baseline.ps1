# Windows counterpart of check-x86-baseline.sh (MSVC has no objdump): llama.cpp's core DLLs (llama, ggml,
# ggml-base) and the SSE2 CPU backend module (ggml-cpu-x64.dll) contain no code above SSE2 outside code that MSVC
# itself guards with a run-time CPU check (__isa_available, _Avx2WmemEnabled); every other CPU backend module
# (B-49) stays within its x86 level and its CPU score code within SSE2; every llama.cpp CMake cache keeps the
# global x86 SIMD options off. The checks are build/check_x86_levels.py's, on `dumpbin /disasm` output.
#   pwsh build/check-x86-baseline.ps1 target/release [-Also <staged or installed folder>] [-RequireModules]
param([string]$Root = 'target', [string[]]$Also = @(), [switch]$RequireModules)
$ErrorActionPreference = 'Stop'

$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
$dumpbin = & $vswhere -latest -products * -find 'VC\Tools\MSVC\**\bin\Hostx64\x64\dumpbin.exe' | Select-Object -First 1
if (-not $dumpbin) { Write-Output 'dumpbin.exe not found (Visual Studio C++ tools)'; exit 1 }

$script = Join-Path $PSScriptRoot 'check_x86_levels.py'
$argv = @('-I', $script, '--tool', 'dumpbin', '--dumpbin', $dumpbin, '--cmake')
if ($RequireModules) { $argv += '--require-modules' }
$argv += $Root
$argv += $Also
& python @argv
exit $LASTEXITCODE
