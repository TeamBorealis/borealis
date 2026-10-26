#!/usr/bin/env bash
# The x86_64 builds must run on every x86_64 CPU (X86-1, local-search-spec sec. 7). llama.cpp's core libraries
# (llama, ggml, ggml-base: static .a or, on x86_64 Linux / Windows, shared .so) use only the SSE2 baseline; x86_64
# Linux / Windows builds also carry one CPU backend module per x86 level (B-49, GGML_CPU_ALL_VARIANTS), and each
# must stay within its level (ggml loads only the module the CPU can run) and its CPU score code within SSE2.
# Details and the level table: build/check_x86_levels.py. Uses objdump (binutils). Run after a build, on the build
# folder and on any folder of staged or installed libraries:
#   bash build/check-x86-baseline.sh [--require-modules] target/debug [more folders...]
# On an x86_64 host it also checks that no llama.cpp CMake cache turned a global x86 SIMD option on (ARM builds
# ignore those options).
set -euo pipefail
case "$(uname -m)" in x86_64 | amd64 | AMD64) cmake=(--cmake) ;; *) cmake=() ;; esac
if [ "$#" -eq 0 ]; then set -- target; fi
exec python3 -I "$(dirname "$0")/check_x86_levels.py" --tool objdump ${cmake[@]+"${cmake[@]}"} "$@"
