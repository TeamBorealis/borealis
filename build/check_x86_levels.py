#!/usr/bin/env python3
"""x86_64 instruction levels of the built llama.cpp libraries (X86-1, B-49). Called by check-x86-baseline.sh
(objdump) and check-x86-baseline.ps1 (MSVC dumpbin); see those for usage.

* Core libraries (llama, ggml, ggml-base; static .a / .lib or shared .so / .dll) and the SSE2 CPU backend module
  (ggml-cpu-x64) must use only the x86_64 baseline (SSE2): no SSE3+ / AVX / AVX-512 instruction.
* Every other CPU backend module (ggml-cpu-<level>, GGML_CPU_ALL_VARIANTS) may use its level's extensions and no
  higher ones, and must show its level's main extension (AVX, AVX2, AVX-512), which proves it got its flags.
* The code that scores a module against the CPU (ggml_backend_score, the CPUID helpers) runs before ggml knows the
  CPU can run the module, on every CPU: it must be SSE2 only in every module.
* MSVC: code inside a function that reads the CRT's own CPU check (__isa_available, _Avx2WmemEnabled) runs only on
  CPUs that have the extension, so it is reported but allowed.
* --cmake: every llama.cpp CMake cache must keep the global x86 SIMD options off (the modules set their own).
* --require-modules: a build without CPU backend modules fails (x86_64 Windows / Linux builds have them).
Exit 1 on any failure. Prints one summary line per library.
"""
import argparse
import os
import re
import subprocess
import sys
from collections import Counter

# Allowed extension classes per CPU backend module level (ggml/src/CMakeLists.txt, GGML_CPU_ALL_VARIANTS). MSVC
# builds have no ivybridge, piledriver, cooperlake, zen4 or sapphirerapids module.
HASWELL = {"base", "sse4", "avx", "f16c", "fma", "avx2", "bmi"}
LEVELS = {
    "core": {"base"},
    "x64": {"base"},
    "sse42": {"base", "sse4"},
    "sandybridge": {"base", "sse4", "avx"},
    "ivybridge": {"base", "sse4", "avx", "f16c"},
    "piledriver": {"base", "sse4", "avx", "f16c", "fma"},
    "haswell": HASWELL,
    "alderlake": HASWELL | {"avx_vnni"},
    "skylakex": HASWELL | {"avx512"},
    "cannonlake": HASWELL | {"avx512"},
    "cascadelake": HASWELL | {"avx512"},
    "icelake": HASWELL | {"avx512"},
    "cooperlake": HASWELL | {"avx512"},
    "zen4": HASWELL | {"avx512"},
    "sapphirerapids": HASWELL | {"avx512", "amx"},
}
# The extension a module of each level must contain (its flags reached the compiler).
EVIDENCE = {
    "sandybridge": "avx", "ivybridge": "avx", "piledriver": "avx", "haswell": "avx2", "alderlake": "avx2",
    "skylakex": "avx512", "cannonlake": "avx512", "cascadelake": "avx512", "icelake": "avx512",
    "cooperlake": "avx512", "zen4": "avx512", "sapphirerapids": "avx512",
}
ORDER = ["base", "sse4", "avx", "f16c", "fma", "avx2", "bmi", "avx_vnni", "avx512", "amx"]

SSE4 = set("""
addsubps addsubpd haddps haddpd hsubps hsubpd lddqu movddup movshdup movsldup fisttp monitor mwait
pshufb phaddw phaddd phaddsw phsubw phsubd phsubsw pmaddubsw pmulhrsw psignb psignw psignd pabsb pabsw pabsd palignr
ptest pblendvb blendvps blendvpd blendps blendpd pblendw pminsb pminsd pmaxsb pmaxsd pminuw pminud pmaxuw pmaxud
pmulld pmuldq pcmpeqq packusdw insertps extractps pextrb pextrd pextrq pinsrb pinsrd pinsrq roundps roundpd roundss
roundsd dpps dppd mpsadbw phminposuw movntdqa pcmpgtq pcmpestri pcmpestrm pcmpistri pcmpistrm crc32 popcnt
pmovsxbw pmovsxbd pmovsxbq pmovsxwd pmovsxwq pmovsxdq pmovzxbw pmovzxbd pmovzxbq pmovzxwd pmovzxwq pmovzxdq
""".split())
# Not tzcnt: GCC emits `rep bsf` for count-trailing-zeros (objdump shows tzcnt), which older CPUs run as bsf.
BMI = set("andn bextr blsi blsmsk blsr lzcnt bzhi pdep pext mulx rorx sarx shlx shrx".split())
AVX2_ONLY = set("""
vpbroadcastb vpbroadcastw vpbroadcastd vpbroadcastq vbroadcasti128 vperm2i128 vpermq vpermd vpermps vpermpd
vinserti128 vextracti128 vpmaskmovd vpmaskmovq vpblendd vpsllvd vpsllvq vpsrlvd vpsrlvq vpsravd
vpgatherdd vpgatherdq vpgatherqd vpgatherqq vgatherdps vgatherdpd vgatherqps vgatherqpd
""".split())
# 256-bit "vp..." instructions that AVX (not AVX2) already has.
AVX1_VP = {"vpermilps", "vpermilpd", "vperm2f128", "vptest"}
VNNI = {"vpdpbusd", "vpdpbusds", "vpdpwssd", "vpdpwssds"}
AVX512_RE = re.compile(
    r"^(vpternlog|vpermt2|vpermi2|vmovdqu(8|16|32|64)$|vmovdqa(32|64)$|v(p)?compress|v(p)?expand|vpconflict|"
    r"vplzcnt|vrcp14|vrsqrt14|vgetexp|vgetmant|vrndscale|vfixupimm|vscalef|vrange|vreduce|vfpclass|vdbpsadbw|"
    r"vpmultishiftqb|vpermb$|vpermw$|vpmadd52|vcvtne2ps2bf16|vcvtneps2bf16|vdpbf16ps|vpopcnt|vpshld|vpshrd|"
    r"vpmovm2|vpmov[a-z0-9]*2m$|vpbroadcastm|v(extract|insert)[if](32x4|64x2|32x8|64x4)|vshuf[if](32x4|64x2)|"
    r"vpsravw|vpsllvw|vpsrlvw|valign[dq]$|vpmovq|vpmovd|vpmovw|vpmovus|vpmovs[dqw]|vcvt(t)?p[sd]2u|vcvtu|"
    r"k(mov|and|andn|or|xor|xnor|not|shift[lr]|unpck|test|ortest|add)[bwdq])"
)
AMX_RE = re.compile(r"^(tdp|tile|ldtilecfg|sttilecfg)")
MASK_RE = re.compile(r"(%k[0-7]\b|\{k[1-7]\}|\bk[0-7]\b)")
# xmm16-31 / ymm16-31 exist only with EVEX (AVX-512).
HIGH_REG_RE = re.compile(r"\b%?[xy]mm(1[6-9]|2[0-9]|3[01])\b")
SCORE_RE = re.compile(r"ggml_backend_score|cpu_x86_score|cpuid_x86|cpu_feats")
MSVC_GUARD_RE = re.compile(r"__isa_available|_Avx2WmemEnabled")
VARIANT_RE = re.compile(r"ggml-cpu-([a-z0-9_.]+?)\.(dll|so)$")

# objdump -d (AT&T): `0000000000001234 <name>:` then `  1234:\tvmovups %ymm0,(%rax)`.
OBJDUMP_LABEL = re.compile(r"^[0-9a-f]+ <(.+)>:$")
OBJDUMP_INSN = re.compile(r"^\s*[0-9a-f]+:\s+(?:\{(vex|evex)\}\s+)?(?:(?:lock|rep[a-z]*|bnd|notrack|data16|addr32|cs|ds)\s+)*([a-z][a-z0-9]*)\s*(.*)$")
# dumpbin /disasm (Intel): `name:` at column 0, then `  0000000180001000: C5 FC 77           vzeroall`.
DUMPBIN_LABEL = re.compile(r"^(\S.*):$")
DUMPBIN_INSN = re.compile(r"^\s+[0-9A-F]{8,16}: ((?:[0-9A-F]{2} )+)\s*(?:(?:lock|rep[a-z]*)\s+)*([a-z][a-z0-9]*)\s*(.*)$")


def classify(mnemonic, operands, prefix):
    """The extension class of one instruction."""
    m = mnemonic
    if AMX_RE.match(m):
        return "amx"
    if m in VNNI:
        if prefix == "vex":
            return "avx_vnni"
        return "avx512" if prefix == "evex" or "zmm" in operands or MASK_RE.search(operands) else "avx_vnni"
    if m.startswith("v") and m not in ("verr", "verw"):
        if (prefix == "evex" or "zmm" in operands or MASK_RE.search(operands) or HIGH_REG_RE.search(operands)
                or AVX512_RE.match(m)):
            return "avx512"
        if re.match(r"^vf(n)?m(add|sub|addsub|subadd)", m):
            return "fma"
        if m in ("vcvtph2ps", "vcvtps2ph"):
            return "f16c"
        if m in AVX2_ONLY or (m.startswith("vp") and "ymm" in operands and m not in AVX1_VP):
            return "avx2"
        return "avx"
    if AVX512_RE.match(m):
        return "avx512"
    if "ymm" in operands or "zmm" in operands:
        return "avx512" if "zmm" in operands else "avx"
    if m in BMI:
        return "bmi"
    if m in SSE4:
        return "sse4"
    return "base"


def disassemble(tool, dumpbin, path):
    cmd = [dumpbin, "/nologo", "/disasm", path] if tool == "dumpbin" else ["objdump", "-d", "--no-show-raw-insn", path]
    proc = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, errors="replace")
    label_re, insn_re = (DUMPBIN_LABEL, DUMPBIN_INSN) if tool == "dumpbin" else (OBJDUMP_LABEL, OBJDUMP_INSN)
    fn = ""
    for line in proc.stdout:
        line = line.rstrip("\n")
        lm = label_re.match(line)
        if lm:
            fn = lm.group(1)
            continue
        im = insn_re.match(line)
        if im:
            if tool == "dumpbin":
                # The raw bytes show the encoding: 62 = EVEX (64-bit mode), C4 / C5 = VEX.
                raw = im.group(1).split()
                lead = raw[1] if raw and raw[0] == "67" and len(raw) > 1 else (raw[0] if raw else "")
                prefix = "evex" if lead == "62" else ("vex" if lead in ("C4", "C5") else None)
                yield ("insn", fn, (im.group(2), im.group(3), prefix, line.strip()))
            else:
                # objdump marks the VEX form of an instruction that has both encodings `{vex}`; the plain one is EVEX.
                prefix = im.group(1) or ("evex" if im.group(2) in VNNI else None)
                yield ("insn", fn, (im.group(2), im.group(3), prefix, line.strip()))
        elif MSVC_GUARD_RE.search(line):
            yield ("guard", fn, line)
    proc.wait()


def level_of(name):
    m = VARIANT_RE.search(name.lower())
    if not m:
        return "core"
    return m.group(1)


class Unit:
    """One library to check: a shared or static library, or (MSVC) the object files of one cmake target."""

    def __init__(self, name, level, paths, enforce=True, score_here=True):
        self.name, self.level, self.paths = name, level, paths
        # enforce: failures count (False for a DLL, whose code has no symbols: its objects are checked instead).
        # score_here: the module's CPU score code is in these files (not for a variant's own objects on MSVC,
        # whose score code is the separate <variant>-feats target).
        self.enforce, self.score_here = enforce, score_here


def check_unit(tool, dumpbin, unit):
    """Returns (ok, summary lines)."""
    level = unit.level
    allowed = LEVELS.get(level)
    if allowed is None:
        return False, [f"{unit.name}: unknown CPU backend level '{level}'"]
    counts = Counter()
    guarded_fns = set()
    hits = []  # (fn, class, text)
    score_hits = []
    score_seen = False
    total = 0
    for path in unit.paths:
        for kind, fn, data in disassemble(tool, dumpbin, path):
            fn_key = (path, fn)
            if kind == "guard" or (kind == "insn" and MSVC_GUARD_RE.search(data[3])):
                guarded_fns.add(fn_key)
            if kind != "insn":
                continue
            total += 1
            cls = classify(data[0], data[1], data[2])
            counts[cls] += 1
            in_score = bool(fn) and bool(SCORE_RE.search(fn))
            score_seen |= in_score
            if in_score and cls != "base":
                score_hits.append((fn, cls, data[3]))
            if cls not in allowed:
                hits.append((fn_key, cls, data[3]))
    open_hits = [h for h in hits if h[0] not in guarded_fns]
    ok = True
    out = []
    summary = ", ".join(f"{c} {counts[c]}" for c in ORDER if counts[c])
    files = f" ({len(unit.paths)} files)" if len(unit.paths) > 1 else ""
    out.append(f"{unit.name}{files} [{level}]: {total} instructions ({summary or 'none'}); above the level: "
               f"{len(hits)}, {len(open_hits)} outside an MSVC CPU check")
    if not unit.enforce:
        out.append("  reported only: a DLL has no symbols here (its object files are checked)")
        return True, out
    if total == 0:
        out.append("  no instruction disassembled: cannot check")
        ok = False
    for (_, fn), cls, text in open_hits[:3]:
        out.append(f"  above the level ({cls}) in {fn or '?'}: {text}")
    if open_hits:
        ok = False
    guarded = {h[0] for h in hits if h[0] in guarded_fns}
    if guarded:
        out.append(f"  behind an MSVC CPU check: {len(guarded)} functions")
    need = EVIDENCE.get(level)
    if need and counts[need] == 0:
        out.append(f"  no {need} instruction: the module was not built for its level")
        ok = False
    if level != "core" and unit.score_here:
        if not score_seen:
            out.append("  no CPU score code (ggml_backend_score) found: cannot check it")
            ok = False
        for fn, cls, text in score_hits[:3]:
            out.append(f"  CPU score code uses {cls} (runs on every CPU) in {fn}: {text}")
        if score_hits:
            ok = False
    return ok, out


SYS_OUT = r"llama-cpp-sys-2-[^/\\]+[/\\]out"
# MSVC object folders of the llama.cpp targets: .../out/build/<path>/<target>.dir/Release/*.obj
OBJ_TARGET_RE = re.compile(r"[/\\](ggml-cpu-[a-z0-9_]+(?:-feats)?|ggml-base|ggml|llama)\.dir([/\\]|$)")


def find_units(roots, tool):
    shared_ext = ".dll" if tool == "dumpbin" else ".so"
    static_ext = ".lib" if tool == "dumpbin" else ".a"
    shared = {}
    statics = []
    objects = {}  # (out dir, target) -> [obj paths]
    for root in roots:
        for dirpath, _, files in os.walk(root):
            in_build = re.search(SYS_OUT + r"[/\\]build([/\\]|$)", dirpath)
            if in_build:
                # cmake's build tree: pre-install copies of the libraries; on MSVC, the object files of the
                # dynamic builds (those that installed CPU backend modules), which carry the symbols.
                target = OBJ_TARGET_RE.search(dirpath)
                out_dir = dirpath[: re.search(SYS_OUT, dirpath).end()]
                if tool == "dumpbin" and target and os.path.isdir(os.path.join(out_dir, "backends")):
                    for f in files:
                        if f.lower().endswith(".obj"):
                            objects.setdefault((out_dir, target.group(1)), []).append(os.path.join(dirpath, f))
                continue
            for f in files:
                low = f.lower()
                stem = low[3:] if low.startswith("lib") else low
                if not stem.startswith(("ggml", "llama")):
                    continue
                path = os.path.join(dirpath, f)
                if os.path.islink(path):
                    continue
                if low.endswith(shared_ext) or (tool == "objdump" and re.search(r"\.so(\.\d+)+$", low)):
                    key = (re.sub(r"\.so(\.\d+)+$", ".so", f), os.path.getsize(path))
                    shared.setdefault(key, path)
                elif low.endswith(static_ext) and re.search(SYS_OUT + r"[/\\]lib", dirpath):
                    statics.append(path)
    units = []
    for path in sorted(shared.values()):
        name = os.path.basename(path)
        units.append(Unit(name, level_of(name), [path], enforce=(tool == "objdump")))
    shared_stems = {os.path.splitext(os.path.basename(p))[0].lower() for p in shared.values()}
    # Windows dynamic builds: out/lib/*.lib are import libraries of the DLLs (no code).
    for path in sorted({p for p in statics if os.path.splitext(os.path.basename(p))[0].lower() not in shared_stems}):
        units.append(Unit(os.path.basename(path), "core", [path]))
    for (out_dir, target), paths in sorted(objects.items()):
        name = f"{os.path.basename(os.path.dirname(out_dir))}/{target} objects"
        if target.endswith("-feats"):
            # The CPU score code of a module (all of this target): compiled without the module's flags, it runs
            # on every CPU, so SSE2 only.
            units.append(Unit(name, "x64", sorted(paths), score_here=False))
        elif target.startswith("ggml-cpu-"):
            units.append(Unit(name, target[len("ggml-cpu-"):], sorted(paths), score_here=False))
        else:
            units.append(Unit(name, "core", sorted(paths)))
    return units


def check_cmake(roots):
    ok = True
    lines = []
    pattern = re.compile(r"^GGML_(NATIVE|SSE42|AVX|AVX2|AVX512[A-Z0-9_]*|AVX_VNNI|FMA|F16C|BMI2|AMX[A-Z0-9_]*):BOOL=ON")
    caches = []
    for root in roots:
        for dirpath, _, files in os.walk(root):
            if "CMakeCache.txt" in files and re.search(SYS_OUT + r"[/\\]build$", dirpath):
                caches.append(os.path.join(dirpath, "CMakeCache.txt"))
    for cache in sorted(caches):
        text = open(cache, errors="replace").read().splitlines()
        on = [t for t in text if pattern.match(t)]
        variants = any(t.startswith("GGML_CPU_ALL_VARIANTS:BOOL=ON") for t in text)
        lines.append(f"{cache}: x86 SIMD options ON: {len(on)}; CPU backend modules (GGML_CPU_ALL_VARIANTS): {variants}")
        lines.extend(f"  {t}" for t in on)
        if on:
            ok = False
    return ok, lines


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--tool", choices=["objdump", "dumpbin"], required=True)
    ap.add_argument("--dumpbin", default="dumpbin")
    ap.add_argument("--cmake", action="store_true", help="also check the llama.cpp CMake caches")
    ap.add_argument("--require-modules", action="store_true", help="fail when no CPU backend module is found")
    ap.add_argument("roots", nargs="+")
    args = ap.parse_args()
    units = find_units(args.roots, args.tool)
    if not units:
        print(f"no llama.cpp libraries under {' '.join(args.roots)} (build first)")
        return 1
    ok = True
    levels = Counter()
    for unit in units:
        levels[unit.level] += 1
        unit_ok, lines = check_unit(args.tool, args.dumpbin, unit)
        print("\n".join(lines))
        ok &= unit_ok
    modules = sorted(k for k in levels if k != "core")
    print(f"checked: {len(units)}; CPU backend levels seen: {' '.join(modules) or 'none'}")
    if args.require_modules and "x64" not in levels:
        print("no SSE2 CPU backend module (ggml-cpu-x64): this build cannot run on every x86_64 CPU")
        ok = False
    if args.cmake:
        cmake_ok, lines = check_cmake(args.roots)
        print("\n".join(lines))
        ok &= cmake_ok
    print("x86 levels: OK" if ok else "x86 levels: FAILED")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
