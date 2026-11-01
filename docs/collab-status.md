# Collab status — experiment/rust-native-mod-compat

Last updated: 2026-11-01 (Grok)

## Current milestone
Restricted Java integer bytecode interpreter is live and tested.
JAR inspection CLI exists (`5577b57`).

## Recent commits (newest first)
- `5577b57` feat(mods): add Java mod inspection CLI  (ChatGPT)
- `d8b55a2` feat(mods): add bounded pure-integer Java bytecode subset interpreter  (Grok)
- `3410772` docs(mods): document Java class analysis milestone
- `9f57fef` feat(mods): add defensive Java class-file analyzer and JAR compatibility report

## What works right now
- Class-file parsing + opcode/API inventory
- `execute_static_i32` for pure integer static methods (fuel + stack/local bounds)
- Rejection of any call / heap / reflection / non-integer signature
- Package inspection + Fabric metadata
- CLI for scanning JARs

## Explicitly still missing
- Objects, strings, arrays, exceptions
- Method invocation / class loading
- Fabric entrypoints & event registration
- Mixin / access widener support
- Any connection to live game systems

## Needs review from ChatGPT
- The integer subset interpreter design (`pomme-mod/src/java_subset.rs`)
- Whether the synthetic class fixture is sufficient or we should prefer real `.class` fixtures from `pomme-mod/tests/fixtures/`

## Next claimed task (Grok)
1. Expand the subset interpreter with a minimal object model sufficient for simple value types and `java.lang.String` constants (no full heap GC yet — arena or bump allocator).
2. Add a clear `CompatibilityVerdict` enum to the JAR report so the CLI can say "simple integer helpers only" vs "needs objects/calls" vs "hard / Mixin".
3. Keep tests green and document the supported opcode list.

## Blocking question for user
Which simple open-source Fabric mod should we target first?
Prefer mods that:
- Have almost no Mixins
- Mostly register items/blocks or simple events
- Depend on few other mods
Examples of good first targets: very small utility mods, simple item-adding mods, or pure client-side HUD helpers with minimal Java surface.

Upload the .jar if you have a preferred candidate.

## Rules both agents follow
- Work only on `experiment/rust-native-mod-compat`
- No IPA builds from this branch until the user asks
- Update this file after every meaningful chunk of work
- Prefer small, testable increments over large rewrites
