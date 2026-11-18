// Makes the OpenUI golden files the Rust parser is checked against (src/openui/tests.rs), with
// OpenUI's own parser and component library: @openuidev/lang-core 1.1.0 and
// @openuidev/react-ui 0.18.6. From the repository's root:
//
//   mkdir /tmp/openui-golden && (cd /tmp/openui-golden && npm init -y && \
//     npm install --ignore-scripts @openuidev/lang-core@1.1.0 @openuidev/react-ui@0.18.6 \
//     @openuidev/react-lang@1.1.0 @openuidev/react-headless@0.18.6 react@19.3.0 react-dom@19.3.0 zod@4.6.7 zustand@4.5.7)
//   DO_NOT_TRACK=1 node tests/openui/golden.mjs /tmp/openui-golden/node_modules .
//
// It writes two things:
// - src/openui/library.json: each component's positional parameters (name, required, JSON
//   schema with defaults) and its two-way ($binding) props, from the library itself, so the Rust
//   side maps and validates arguments exactly as lang-core does. Object properties are written as
//   [key, schema] pairs because their order is the positional order and the order errors come in.
// - tests/openui/<name>.json for each tests/openui/<name>.oui: what lang-core parsed (errors,
//   unresolved and orphaned names, state defaults) and the root as the web first draws it,
//   evaluated against those defaults. Fields that only steer React (partial, hasDynamicProps,
//   statementId) and mappedProps inside unevaluated expressions are left out.
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const [mods, core] = process.argv.slice(2);
if (!mods || !core) throw new Error("usage: node golden.mjs <node_modules> <repository root>");
const { openuiLibrary } = await import(`${mods}/@openuidev/react-ui/dist/genui-lib/index.mjs`);
const lang = await import(`${mods}/@openuidev/lang-core/dist/index.mjs`);

const schema = openuiLibrary.toJSONSchema();
const known = new Set(["type", "enum", "const", "properties", "required", "items", "anyOf", "$ref", "default", "description", "additionalProperties", "minItems", "maxItems", "propertyNames"]);

// The schema as the Rust side reads it: action refs inlined (lang-core's compileSchema), no
// descriptions, properties in order.
function compact(s, path) {
  if (Array.isArray(s)) return s.map((x, i) => compact(x, `${path}/${i}`));
  if (!s || typeof s !== "object") return s;
  if (typeof s.$ref === "string" && s.$ref.endsWith("/ActionExpression")) return compact(schema.$defs.ActionExpression, path);
  const out = {};
  for (const [k, v] of Object.entries(s)) {
    if (!known.has(k)) throw new Error(`${path}: schema keyword ${k} isn't handled by the Rust side`);
    if (k === "description" || k === "additionalProperties" || k === "propertyNames" || k === "minItems" || k === "maxItems") continue;
    if (k === "properties") out.properties = Object.entries(v).map(([pk, pv]) => [pk, compact(pv, `${path}/${pk}`)]);
    else if (k === "default" || k === "enum" || k === "const") out[k] = v;
    else out[k] = compact(v, `${path}/${k}`);
  }
  return out;
}

const components = {};
for (const name of Object.keys(schema.properties)) {
  const def = schema.$defs[name];
  const required = def.required ?? [];
  const reactive = Object.entries(openuiLibrary.components[name].props.shape).filter(([, s]) => lang.isReactiveSchema(s)).map(([k]) => k);
  components[name] = {
    params: Object.entries(def.properties ?? {}).map(([k, s]) => ({ name: k, required: required.includes(k), schema: compact(s, `${name}/${k}`) })),
    ...(reactive.length ? { reactive } : {}),
  };
}
if (schema.functions || schema.actions) throw new Error("the library has functions or actions now; the Rust side doesn't know them");
writeFileSync(join(core, "src/openui/library.json"), JSON.stringify({ root: openuiLibrary.root, components }) + "\n");

// An unevaluated expression (a @Set step's value) without the parser's argument mapping, which
// the Rust side keeps elsewhere.
function bareAST(n) {
  if (Array.isArray(n)) return n.map(bareAST);
  if (!n || typeof n !== "object") return n;
  const out = {};
  for (const [k, v] of Object.entries(n)) if (k !== "mappedProps") out[k] = bareAST(v);
  return out;
}

const kinds = new Set(["Comp", "Ref", "StateRef", "RuntimeRef", "BinOp", "UnaryOp", "Ternary", "Member", "Index", "Assign", "Str", "Num", "Bool", "Null", "Arr", "Obj", "Ph"]);

function shown(v) {
  if (Array.isArray(v)) return v.map(shown);
  if (!v || typeof v !== "object") return typeof v === "number" && !Number.isFinite(v) ? null : v;
  if (lang.isElementNode(v)) return { type: "element", typeName: v.typeName, props: shown(v.props) };
  if (kinds.has(v.k)) return bareAST(v);
  const out = {};
  for (const [k, x] of Object.entries(v)) {
    if (k === "valueAST") out[k] = bareAST(x);
    else if (x !== undefined && typeof x !== "function") out[k] = shown(x);
  }
  return out;
}

const parser = lang.createParser(schema, openuiLibrary.root);
const dir = join(core, "tests/openui");
let n = 0;
for (const file of readdirSync(dir).filter((f) => f.endsWith(".oui")).sort()) {
  const src = readFileSync(join(dir, file), "utf8");
  const r = parser.parse(src);
  const store = lang.createStore();
  store.initialize(r.stateDeclarations, {});
  const errors = [];
  const ctx = { getState: (name) => store.get(name), resolveRef: () => null };
  let root = null;
  try {
    root = lang.evaluateRoot(r, { ctx, library: openuiLibrary, store, errors });
  } catch (e) {
    errors.push({ code: "threw", message: String(e) });
  }
  const out = {
    parsed: r.root !== null,
    root: shown(root),
    errors: r.meta.errors.map((e) => ({ code: e.code, component: e.component, path: e.path, statementId: e.statementId ?? null, ...(e.severity ? { severity: e.severity } : {}) })),
    unresolved: r.meta.unresolved,
    orphaned: r.meta.orphaned,
    incomplete: r.meta.incomplete,
    statementCount: r.meta.statementCount,
    state: shown(r.stateDeclarations),
  };
  writeFileSync(join(dir, file.replace(/\.oui$/, ".json")), JSON.stringify(out, null, 1) + "\n");
  n++;
}
console.log(`wrote library.json and ${n} golden files`);
