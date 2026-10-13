// Generates src/api/generated/contract.json from src/api/contract.ts using the TypeScript compiler:
// the command list, each command's argument keys and the structural shape of every response and event.
// The Rust test `src-tauri/tests/contract.rs` checks the real command layer against this file.
// Usage: node scripts/gen-contract.mjs [--check]
import ts from "typescript";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const OUT = path.join(root, "src/api/generated/contract.json");

export function generate() {
  const cfgPath = path.join(root, "tsconfig.json");
  const cfg = ts.getParsedCommandLineOfConfigFile(cfgPath, {}, { ...ts.sys, onUnRecoverableConfigFileDiagnostic: () => {} });
  const program = ts.createProgram([path.join(root, "src/api/contract.ts")], cfg.options);
  const checker = program.getTypeChecker();
  const file = program.getSourceFile(path.join(root, "src/api/contract.ts"));
  const defs = {};
  const F = ts.TypeFlags;

  const shape = (type, seen = new Set()) => {
    const f = type.flags;
    if (f & (F.Any | F.Unknown)) return { k: "any" };
    if (f & (F.String | F.Number | F.Boolean | F.StringLiteral | F.NumberLiteral | F.BooleanLiteral | F.BigInt | F.Enum | F.EnumLiteral | F.TemplateLiteral)) return { k: "prim" };
    if (f & (F.Null | F.Undefined | F.Void | F.Never)) return { k: "nil" };
    if (type.isUnion()) {
      const parts = type.types.map((t) => shape(t, seen)).filter((s) => s.k !== "nil");
      const flat = parts.filter((s, i) => parts.findIndex((p) => JSON.stringify(p) === JSON.stringify(s)) === i);
      if (flat.length === 0) return { k: "nil" };
      return flat.length === 1 ? flat[0] : { k: "union", variants: flat };
    }
    if (type.isIntersection()) {
      const props = {};
      for (const p of checker.getPropertiesOfType(type)) props[p.name] = prop(p, seen);
      return { k: "object", props };
    }
    if (checker.isArrayType(type) || checker.isTupleType(type)) {
      const args = checker.getTypeArguments(type);
      const el = args.length === 1 ? shape(args[0], seen) : { k: "any" };
      return { k: "array", t: el };
    }
    const idx = type.getStringIndexType?.();
    if (idx && checker.getPropertiesOfType(type).length === 0) return { k: "record", t: shape(idx, seen) };
    if (f & F.Object) {
      const base = type.aliasSymbol?.name ?? type.symbol?.name;
      const name = base && (type.aliasTypeArguments?.length || type.typeArguments?.length || type.target?.typeParameters?.length) ? checker.typeToString(type) : base;
      const named = name && !name.startsWith("__") && type.symbol?.declarations?.some((d) => !d.getSourceFile().isDeclarationFile);
      if (named) {
        if (!defs[name]) {
          defs[name] = { k: "pending" };
          defs[name] = objectShape(type, seen);
        }
        return { k: "ref", name };
      }
      return objectShape(type, seen);
    }
    return { k: "any" };
  };
  const prop = (sym, seen) => {
    const t = checker.getTypeOfSymbolAtLocation(sym, file);
    const opt = !!(sym.flags & ts.SymbolFlags.Optional);
    return { opt, t: shape(opt && !(t.flags & (F.Any | F.Unknown)) ? checker.getNonNullableType(t) : t, seen) };
  };
  const objectShape = (type, seen) => {
    const props = {};
    for (const p of checker.getPropertiesOfType(type)) props[p.name] = prop(p, seen);
    const open = !!type.getStringIndexType?.();
    return open ? { k: "object", props, open: true } : { k: "object", props };
  };

  const methods = checker.getTypeAtLocation(file.statements.find((s) => ts.isInterfaceDeclaration(s) && s.name.text === "ApiMethods"));
  const commandsDecl = file.statements.filter(ts.isVariableStatement).find((s) => s.declarationList.declarations[0].name.getText() === "COMMANDS");
  let init = commandsDecl.declarationList.declarations[0].initializer;
  while (!ts.isObjectLiteralExpression(init)) init = init.expression;
  const lit = init.properties;
  const commands = {};
  for (const p of lit) commands[p.name.getText()] = p.initializer.text;

  const out = { commands: {}, events: {}, defs };
  for (const sym of checker.getPropertiesOfType(methods)) {
    const decl = sym.declarations[0];
    const sig = checker.getSignaturesOfType(checker.getTypeOfSymbolAtLocation(sym, decl), ts.SignatureKind.Call)[0];
    const ret = checker.getAwaitedType(sig.getReturnType()) ?? sig.getReturnType();
    const param = sig.parameters[0];
    let args = {};
    if (param) {
      const pt = checker.getTypeOfSymbolAtLocation(param, decl);
      for (const a of checker.getPropertiesOfType(pt)) args[a.name] = !!(a.flags & ts.SymbolFlags.Optional);
    }
    out.commands[commands[sym.name]] = { method: sym.name, args, response: shape(ret) };
  }
  const events = checker.getTypeAtLocation(file.statements.find((s) => ts.isInterfaceDeclaration(s) && s.name.text === "ApiEvents"));
  for (const sym of checker.getPropertiesOfType(events)) out.events[sym.name] = shape(checker.getTypeOfSymbolAtLocation(sym, file.statements[0]));
  const sortKeys = (v) => Array.isArray(v) ? v.map(sortKeys) : v && typeof v === "object" ? Object.fromEntries(Object.keys(v).sort().map((k) => [k, sortKeys(v[k])])) : v;
  return JSON.stringify(sortKeys(out), null, 1) + "\n";
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const text = generate();
  if (process.argv.includes("--check")) {
    if (fs.readFileSync(OUT, "utf8") !== text) { console.error("generated/contract.json is stale: run npm run gen:contract"); process.exit(1); }
  } else fs.writeFileSync(OUT, text);
}
