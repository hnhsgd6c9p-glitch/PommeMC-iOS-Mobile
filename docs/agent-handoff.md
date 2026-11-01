# Shared agent handoff — Java/Fabric compatibility

_Last updated: 2026-11-01_

## Current branch and baseline

- Branch: `experiment/rust-native-mod-compat`
- Previous shared baseline: `5577b57` (`feat(mods): add Java mod inspection CLI`)
- Current work adds a conservative `CompatibilityVerdict` classifier and shared agent coordination docs.
- Main iOS/IPA branch remains isolated: `ios-ipa-cloud-build`.

## Implemented

- Defensive Java class-file and JAR bytecode inventory (`classfile.rs`).
- Restricted, fuel-metered Rust interpreter for a small static integer-method subset (`java_subset.rs`).
- `pomme-mod-inspect` CLI for Fabric metadata and bytecode triage; it now prints the porting verdict.
- Conservative porting verdicts: `NativeWasm`, `MetadataOnly`, `SimpleJavaCandidate`, `HardJava`, `Unsupported`.
- Shared agent workflow in `docs/agent-collaboration.md` and this handoff file.
- Wasmi prototype for separately packaged native Pomme WASM modules.

## Current hard limitations

- No general JVM, object model, arrays, strings, exceptions, class loader, or general method invocation.
- No Fabric Loader lifecycle, entrypoint construction, event bus, Fabric API implementation, Mixin transformation, or access-widener application.
- No end-to-end Java Fabric mod has been ported or loaded in-game.
- Analyzer does not yet fully inventory field accesses, invokedynamic/bootstrap methods, all annotations/attributes, or every Java API usage. Treat its report as an initial triage signal, not a complete static proof.
- `SimpleJavaCandidate` must never be described as a loadable mod; it only identifies code whose observed opcodes fit the current tiny integer interpreter subset.

## Next work

1. Review and test the compatibility-verdict heuristics introduced alongside this handoff. Ensure each verdict explains why it was assigned and errs toward `HardJava`/`Unsupported` when evidence is incomplete.
2. Add a real `javac --release 8` fixture and tests for branches, constant-pool method/field references, malformed attributes, and unsupported opcodes. Avoid tests that rely only on fabricated class bytes.
3. Expand class-file analysis for field references, `invokedynamic` and BootstrapMethods, annotations, and class/member attributes. Record class-level and method-level feature evidence.
4. Fix resource-accounting gaps in JAR analysis: enforce aggregate uncompressed-byte and class-count budgets, not only per-entry limits.
5. Design the next interpreter milestone around Java objects/strings/arrays and method calls, but do not add these until the bytecode verifier, exception semantics, and bounded heap model are specified.
6. Choose one small, permissively licensed Fabric mod as a real porting target after inspecting its dependency and Mixin footprint.

## Latest local validation

- `cargo fmt --all -- --check`: passed.
- `cargo test -p pomme-mod --locked`: 18 passed.
- `cargo clippy -p pomme-mod --all-targets --locked -- -D warnings`: passed.
- `cargo check -p pomme-mod --target aarch64-apple-ios --locked`: passed.
- `git diff --check`: passed.
- GitHub Actions must still validate the pushed commit; local success is not CI success.

## Review requests for the next contributor

- Check the opcode whitelist against the interpreter's actual supported instructions.
- Check that any verdict involving class versions, static initializers, reflection, native methods, mixins, access wideners, and nested JARs is conservative.
- Review the class-file parser's handling of constant-pool tags, malformed/truncated attributes, and bytecode boundaries.
- Do not mark the branch ready for merge or an IPA build.
