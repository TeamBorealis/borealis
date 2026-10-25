#!/usr/bin/env bash
# The x86_64 builds must run on every x86_64 CPU (S3.3, local-search-spec sec. 7): llama.cpp (llama-cpp-sys-2) is
# compiled with GGML_NATIVE=OFF and enables AVX / AVX2 / FMA / F16C only when Rust's target features have them,
# which the default x86_64 targets do not. This check fails when the built llama.cpp static libraries contain any
# AVX or AVX-512 instruction (VEX / EVEX encoded: a `v...` mnemonic or a ymm / zmm register) or when a CMake cache
# turned an x86 SIMD option on. Run after a build: `bash build/check-x86-baseline.sh target/debug`.
set -euo pipefail
root=${1:-target}
bad=0
found=0
while IFS= read -r lib; do
  found=1
  n=$(objdump -d --no-show-raw-insn "$lib" | grep -cE '%[yz]mm|^[[:space:]]*[0-9a-f]+:[[:space:]]+v[a-z0-9]+[[:space:]]' || true)
  echo "$lib: $n AVX instructions"
  if [ "$n" -ne 0 ]; then bad=1; fi
done < <(find "$root" -path '*llama-cpp-sys-2-*/out/lib*' -name '*.a' \( -name 'libggml*' -o -name 'libllama*' \) | sort)
if [ "$found" -eq 0 ]; then
  echo "no llama.cpp static libraries under $root (build first)"
  exit 1
fi
# The CMake options matter only on x86_64 hosts (ARM builds ignore them).
case "$(uname -m)" in x86_64 | amd64 | AMD64) ;; *) exit "$bad" ;; esac
while IFS= read -r cache; do
  if grep -E '^GGML_(NATIVE|AVX|AVX2|AVX512[A-Z0-9_]*|AVX_VNNI|FMA|F16C|BMI2|AMX[A-Z0-9_]*):BOOL=ON' "$cache"; then
    echo "$cache turns an x86 SIMD option on"
    bad=1
  fi
done < <(find "$root" -path '*llama-cpp-sys-2-*/out/build/CMakeCache.txt')
exit "$bad"
