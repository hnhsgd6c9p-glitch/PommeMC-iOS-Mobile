# Shared agent handoff — Java/Fabric compatibility

_Last updated: 2026-11-01 (Grok review of 806dbc7)_

## Current branch and baseline

- Branch: `experiment/rust-native-mod-compat`
- Reviewed commit: `806dbc7` (`feat(mods): add conservative compatibility verdicts and agent handoff`)
- Follow-up fix commit will align the opcode whitelist with the interpreter.
- Main iOS/IPA branch remains isolated: `ios-ipa-cloud-build`.

## Grok independent review of 806dbc7

### 1. Opcode whitelist vs interpreter (critical)

`compatibility_verdict` used a hard-coded `SUBSET` list that included `"nop"`.
The interpreter in `java_subset.rs` does **not** accept opcode `0x00` (nop).
This was a false-positive path: a JAR whose only observed opcode was `nop`
could be labelled `SimpleJavaCandidate` even though execution would reject it.

**Fix applied:** removed `nop` from the whitelist and added a regression test
`compatibility_verdict_rejects_opcodes_outside_interpreter_subset`.

Actual interpreter-supported opcodes (must stay in sync):

- Constants: `iconst_m1`…`iconst_5`, `bipush`, `sipush`
- Locals: `iload` / `iload_0`…`iload_3`, `istore` / `istore_0`…`istore_3`
- Stack: `pop`, `dup`
- Arithmetic: `iadd`, `isub`, `imul`, `idiv`, `irem`, `ineg`
- Bitwise / shift: `ishl`, `ishr`, `iushr`, `iand`, `ior`, `ixor`, `iinc`
- Branches: `ifeq`…`ifle`, `if_icmpeq`…`if_icmple`, `goto`
- Return: `ireturn` only (void `return` / `0xb1` is not accepted for the i32 helper path)

### 2. Verdict conservatism (good)

The following paths correctly force non-candidate results:

- Any skipped class → `Unsupported`
- High-risk APIs / native methods / class major > 65 → `Unsupported`
- Entrypoints, mixins, access wideners, nested JARs, static initializers,
  reflection, threading, filesystem, network, dynamic class loading → `HardJava`
- Non-empty `invoked_methods` → falls through to `HardJava` (cannot be Simple)

No false “loadable mod” claim is made. Documentation correctly states that
`SimpleJavaCandidate` only means observed opcodes fit the tiny integer subset.

### 3. Class-file parser gaps (still open)

- Fields are counted only; `getfield` / `putfield` / `getstatic` / `putstatic`
  owners are not aggregated into the report.
- `invokedynamic` is visible only as an opcode name; the `BootstrapMethods`
  attribute is skipped by length and never inspected.
- Annotation counting is partial (RuntimeVisible/InvisibleAnnotations only).
- Constant-pool tags for MethodHandle / MethodType / Dynamic are stored as
  `Other` and not surfaced in the inventory.

These gaps make the report an **initial triage signal**, not a complete static
proof. The handoff already notes this; keep it conservative.

### 4. Archive resource limits (still open)

Current limits:

- Max archive entries: 10_000
- Max per-class bytes: 32 MiB
- Max manifest bytes: 1 MiB

Missing:

- Aggregate uncompressed size budget across all class entries
- Explicit total class-count budget independent of ZIP entry count

A crafted JAR with many medium-sized classes can still expand memory more than
intended. Recommend adding a running uncompressed-byte counter in
`analyze_fabric_jar` / `analyze_jar_classes`.

### 5. Fixtures

Most interpreter tests still use hand-built class bytes. There is a
`pomme-mod/tests/fixtures/Arithmetic.java` source file but it is not yet wired
into automated tests via `javac --release 8`. Next implementation pass should
compile that fixture (or a new one) and assert both analysis and execution.

## Implemented (cumulative)

- Defensive Java class-file and JAR bytecode inventory (`classfile.rs`).
- Restricted, fuel-metered Rust interpreter for a small static integer-method subset (`java_subset.rs`).
- `pomme-mod-inspect` CLI for Fabric metadata and bytecode triage with verdict.
- Conservative porting verdicts: `NativeWasm`, `MetadataOnly`, `SimpleJavaCandidate`, `HardJava`, `Unsupported`.
- Whitelist now aligned with the interpreter (nop removed).
- Shared agent workflow docs.

## Current hard limitations

- No general JVM, object model, arrays, strings, exceptions, class loader, or general method invocation.
- No Fabric Loader lifecycle, entrypoint construction, event bus, Fabric API, Mixin, or access-widener support.
- No end-to-end Java Fabric mod has been ported or loaded in-game.
- `SimpleJavaCandidate` is **not** a loadable mod.

## Next work (recommended order)

1. **ChatGPT review request:** confirm whitelist alignment and decide whether
   void `return` (0xb1) should be accepted for pure helpers that return void.
2. Add aggregate uncompressed-byte + class-count budgets in JAR analysis.
3. Inventory field references and flag `invokedynamic` / BootstrapMethods.
4. Wire a real `javac --release 8` fixture into tests.
5. Only after the above: design the next interpreter milestone (objects/strings).

## Local validation (this review pass)

- `cargo test -p pomme-mod --locked`: 18 passed before fix; re-run after whitelist change.
- Full fmt / clippy / iOS check will be run with the fix commit.

## Collaboration note

User requested agent-to-agent messages move to the Mac Notes app
(“PommeMC Agent Collab” / “Grok note: …”). Repo handoff files remain the
authoritative engineering record; Notes is for short status pings.
