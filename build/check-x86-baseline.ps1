# Windows counterpart of check-x86-baseline.sh (S3.3 handoff sec. 7 item 3; MSVC has no objdump): the built
# llama.cpp static libraries (ggml-cpu.lib and the rest) must contain no AVX / AVX-512 code (no ymm / zmm register,
# no VEX / EVEX `v`-prefixed mnemonic) outside code that MSVC itself guards with a run-time CPU check, and every CMake cache
# must have GGML_AVX2 OFF and no x86 SIMD option ON.
#   pwsh build/check-x86-baseline.ps1 target/release
param([string]$Root = 'target', [int]$Context = 14)
$ErrorActionPreference = 'Stop'

$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
$dumpbin = & $vswhere -latest -products * -find 'VC\Tools\MSVC\**\bin\Hostx64\x64\dumpbin.exe' | Select-Object -First 1
if (-not $dumpbin) { Write-Output 'dumpbin.exe not found (Visual Studio C++ tools)'; exit 1 }

$libs = @(Get-ChildItem -Path $Root -Recurse -Filter '*.lib' -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match 'llama-cpp-sys-2-[^\\/]+[\\/]out[\\/]lib' -and $_.Name -match '^(ggml|llama)' } |
    Sort-Object FullName)
if ($libs.Count -eq 0) { Write-Output "no llama.cpp static libraries under $Root (build first)"; exit 1 }
if (-not ($libs | Where-Object Name -eq 'ggml-cpu.lib')) { Write-Output "no ggml-cpu.lib under $Root"; exit 1 }

# dumpbin /disasm: a function label at column 0 (`name:`), then `  0000000000000010: C5 FC 77    vzeroall`.
$insn = '^\s+[0-9A-F]{8,16}: (?:[0-9A-F]{2} )+\s*([a-z][a-z0-9]*)'
$label = '^(\S.*):$'
# MSVC's own run-time CPU dispatch: the CRT's `__isa_available` (auto-vectoriser and CRT paths with AVX2 / AVX-512
# variants) and the STL's `_Avx2WmemEnabled` (vectorised wmemchr in std::filesystem). Code behind them runs only on
# CPUs that have the extension (S3.4: Intel SDE -mrm runs these libraries on a Core 2 without an SDE error).
$guard = '__isa_available|_Avx2WmemEnabled'
$bad = 0
foreach ($lib in $libs) {
    $total = 0; $xmm = 0; $isa = 0
    $fn = ''; $recent = New-Object System.Collections.Generic.Queue[string]
    $hits = New-Object System.Collections.Generic.List[object]
    $guarded = @{}
    & $dumpbin /nologo /disasm $lib.FullName | ForEach-Object {
        $line = $_
        if ($line -cmatch $label) { $fn = $Matches[1]; $recent.Clear() }
        if ($line -cmatch $guard) { $isa++; $guarded[$fn] = $true }
        if ($line -cmatch $insn) {
            $total++
            $m = $Matches[1]
            $vex = $m.StartsWith('v') -and $m -notin @('verr', 'verw')
            $wide = $line -cmatch '\b[yz]mm\d'
            if ($vex -or $wide) {
                $hits.Add([pscustomobject]@{ Fn = $fn; Line = $line.Trim(); Before = @($recent.ToArray()) })
            } elseif ($line -cmatch '\bxmm\d') { $xmm++ }
        }
        $recent.Enqueue($line.Trim())
        while ($recent.Count -gt $Context) { [void]$recent.Dequeue() }
    }
    $open = @($hits | Where-Object { -not $guarded[$_.Fn] })
    $fns = @($hits | ForEach-Object Fn | Sort-Object -Unique)
    Write-Output ("{0}: {1} instructions, {2} SSE (xmm); AVX/AVX-512 {3} in {4} functions, {5} outside a block with an MSVC CPU check ({6} checks)" -f `
        $lib.Name, $total, $xmm, $hits.Count, $fns.Count, $open.Count, $isa)
    foreach ($f in $fns) {
        $n = @($hits | Where-Object Fn -eq $f).Count
        Write-Output ("  {0}: {1} AVX ({2})" -f $f, $n, $(if ($guarded[$f]) { 'behind an MSVC CPU check' } else { 'no CPU check in the function' }))
    }
    $show = @($open | Select-Object -First 1)
    if ($show.Count -eq 0) { $show = @($hits | Select-Object -First 1) }
    foreach ($h in $show) {
        Write-Output "  first hits in $($h.Fn): $($h.Line)"
        $h.Before | ForEach-Object { Write-Output "    | $_" }
    }
    if ($total -eq 0) { Write-Output '  no instruction disassembled (LTCG object?): cannot check'; $bad = 1 }
    if ($open.Count -ne 0) { $bad = 1 }
}

$caches = @(Get-ChildItem -Path $Root -Recurse -Filter 'CMakeCache.txt' -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match 'llama-cpp-sys-2-[^\\/]+[\\/]out[\\/]build[\\/]CMakeCache\.txt$' })
if ($caches.Count -eq 0) { Write-Output "no llama.cpp CMake cache under $Root"; exit 1 }
foreach ($c in $caches) {
    $text = Get-Content $c.FullName
    $on = @($text | Select-String -Pattern '^GGML_(NATIVE|AVX|AVX2|AVX512[A-Z0-9_]*|AVX_VNNI|FMA|F16C|BMI2|AMX[A-Z0-9_]*):BOOL=ON')
    $avx2Off = @($text | Select-String -SimpleMatch 'GGML_AVX2:BOOL=OFF').Count -gt 0
    $flags = @($text | Select-String -Pattern '^CMAKE_C(XX)?_FLAGS(_RELEASE|_RELWITHDEBINFO)?:STRING=' | ForEach-Object { $_.Line })
    Write-Output ("{0}: GGML_AVX2 OFF = {1}; x86 SIMD options ON: {2}" -f $c.Directory.Parent.Parent.Name, $avx2Off, $on.Count)
    $flags | ForEach-Object { Write-Output "  $_" }
    $on | ForEach-Object { Write-Output "  $($_.Line)" }
    if (-not $avx2Off -or $on.Count -ne 0) { $bad = 1 }
}
exit $bad
