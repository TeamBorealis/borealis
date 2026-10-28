# B-56: llama.cpp's DLLs (llama, ggml, ggml-base) and CPU backend modules (ggml\ggml-cpu-*.dll) built against the
# "hybrid" MSVC runtime, the same one borealis.exe uses (tauri-build's static vcruntime): the C++ standard library
# (libcpmt) and vcruntime (libvcruntime) linked statically into every module, the Universal CRT (ucrtbase.dll, part
# of Windows 10 and later) dynamically. No module then imports msvcp140.dll or vcruntime140*.dll, so the app starts
# on a PC without the Visual C++ redistributable.
#
# Not the fully static CRT (/MT alone, libucrt): llama.dll opens model files with ggml_fopen() from ggml-base.dll
# and reads, seeks and closes the FILE* with its own CRT (src/llama-mmap.cpp, llama_file), so both DLLs must share
# one C runtime (FILE*, file descriptors, heap, errno). With the dynamic UCRT they do; what is private per module
# (C++ library and vcruntime: exception handling, RTTI, memcpy) never crosses ggml's C API.
#
# Included after every project() of the llama.cpp CMake build: .cargo/config.toml sets CMAKE_PROJECT_INCLUDE, and
# llama-cpp-sys-2 forwards CMAKE_* variables to CMake. A no-op for every compiler but MSVC.
include_guard(GLOBAL)

if(MSVC)
    # /MT (/MTd), in step with LLAMA_STATIC_CRT=1 (the flags llama-cpp-sys-2 hands CMake through cmake-rs).
    set(CMAKE_MSVC_RUNTIME_LIBRARY "MultiThreaded$<$<CONFIG:Debug>:Debug>")
    # The C part of the runtime from ucrtbase.dll (ucrt.lib, the api-ms-win-crt-* imports) instead of libucrt.lib.
    add_link_options(
        "/NODEFAULTLIB:libucrt$<$<CONFIG:Debug>:d>.lib"
        "/DEFAULTLIB:ucrt$<$<CONFIG:Debug>:d>.lib"
    )
endif()
