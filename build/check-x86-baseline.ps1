# Windows counterpart of check-x86-baseline.sh (S3.3 handoff sec. 7 item 3; MSVC has no objdump): the built
# llama.cpp static libraries (ggml-cpu.lib and the rest) must contain no AVX / AVX-512 code (no ymm / zmm register,
# no VEX / EVEX `v`-prefixed mnemonic), and every CMake cache must have GGML_AVX2 OFF and no x86 SIMD option ON.
#   pwsh build/check-x86-baseline.ps1 target/release
param([string]$Root = 'target')
$ErrorActionPreference = 'Stop'

$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
$dumpbin = & $vswhere -latest -products * -find 'VC\Tools\MSVC\**\bin\Hostx64\x64\dumpbin.exe' | Select-Object -First 1
if (-not $dumpbin) { Write-Output 'dumpbin.exe not found (Visual Studio C++ tools)'; exit 1 }

$libs = @(Get-ChildItem -Path $Root -Recurse -Filter '*.lib' -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match 'llama-cpp-sys-2-[^\\/]+[\\/]out[\\/]lib' -and $_.Name -match '^(ggml|llama)' } |
    Sort-Object FullName)
if ($libs.Count -eq 0) { Write-Output "no llama.cpp static libraries under $Root (build first)"; exit 1 }
if (-not ($libs | Where-Object Name -eq 'ggml-cpu.lib')) { Write-Output "no ggml-cpu.lib under $Root"; exit 1 }

# dumpbin /disasm line: `  0000000000000010: C5 FC 77           vzeroall` (address, bytes, mnemonic, operands).
$insn = '^\s+[0-9A-F]{8,16}: (?:[0-9A-F]{2} )+\s*([a-z][a-z0-9]*)'
$bad = 0
foreach ($lib in $libs) {
    $total = 0; $avx = 0; $wide = 0; $xmm = 0; $first = $null
    & $dumpbin /nologo /disasm $lib.FullName | ForEach-Object {
        $line = $_
        if ($line -cmatch $insn) {
            $total++
            if ($Matches[1].StartsWith('v') -and $Matches[1] -notin @('verr', 'verw')) {
                $avx++
                if (-not $first) { $first = $line.Trim() }
            }
            if ($line -cmatch '\b[yz]mm\d') { $wide++; if (-not $first) { $first = $line.Trim() } }
            elseif ($line -cmatch '\bxmm\d') { $xmm++ }
        }
    }
    Write-Output ("{0}: {1} instructions, {2} SSE (xmm), {3} AVX (v-prefixed), {4} with ymm/zmm" -f $lib.Name, $total, $xmm, $avx, $wide)
    if ($total -eq 0) { Write-Output "  no instruction disassembled (LTCG object?): cannot check"; $bad = 1 }
    if ($avx -ne 0 -or $wide -ne 0) { Write-Output "  first: $first"; $bad = 1 }
}

$caches = @(Get-ChildItem -Path $Root -Recurse -Filter 'CMakeCache.txt' -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match 'llama-cpp-sys-2-[^\\/]+[\\/]out[\\/]build[\\/]CMakeCache\.txt$' })
if ($caches.Count -eq 0) { Write-Output "no llama.cpp CMake cache under $Root"; exit 1 }
foreach ($c in $caches) {
    $text = Get-Content $c.FullName
    $on = @($text | Select-String -Pattern '^GGML_(NATIVE|AVX|AVX2|AVX512[A-Z0-9_]*|AVX_VNNI|FMA|F16C|BMI2|AMX[A-Z0-9_]*):BOOL=ON')
    $avx2Off = @($text | Select-String -SimpleMatch 'GGML_AVX2:BOOL=OFF').Count -gt 0
    Write-Output ("{0}: GGML_AVX2 OFF = {1}; x86 SIMD options ON: {2}" -f $c.Directory.Parent.Parent.Name, $avx2Off, $on.Count)
    $on | ForEach-Object { Write-Output "  $($_.Line)" }
    if (-not $avx2Off -or $on.Count -ne 0) { $bad = 1 }
}
exit $bad
