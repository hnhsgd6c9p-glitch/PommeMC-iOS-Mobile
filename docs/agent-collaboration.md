# PommeMC mod-compatibility agent collaboration

This document is the shared coordination protocol for contributors and AI agents working on `experiment/rust-native-mod-compat`.

## Source of truth

- Shared branch: `experiment/rust-native-mod-compat`.
- Shared repository: `hnhsgd6c9p-glitch/PommeMC-iOS-Mobile`.
- Git commits and this repository's issue/PR history are authoritative. Chat messages are not the only record of a decision.
- Keep all Java compatibility work on this branch. Do not merge it into `ios-ipa-cloud-build` or publish an IPA without the user's explicit request.

## Before editing

1. Read this file and `docs/agent-handoff.md`.
2. Fetch/pull the latest branch and inspect `git status`.
3. Read the implementation and tests for the subsystem being changed.
4. Check recent commits for concurrent work. Never reset, force-push, or overwrite another contributor's changes.
5. If another contributor is actively editing the same files, choose a separate file/module or leave a review note rather than racing the edit.

## After editing

1. Run `cargo fmt --all -- --check`, `cargo test -p pomme-mod --locked`, `cargo clippy -p pomme-mod --all-targets --locked -- -D warnings`, `cargo check -p pomme-mod --target aarch64-apple-ios --locked`, and `git diff --check` where the local toolchain permits.
2. Update `docs/agent-handoff.md` with commit, changes, tests, known limitations, and the next review request.
3. Commit and push to the shared branch. Do not claim CI passed until the matching GitHub Actions run has completed successfully.
4. Review the other contributor's commits; report concrete correctness, safety, API, and test concerns with file/line references.

## Division of work

- **Implementation pass:** add a focused feature, tests, and documentation; avoid unrelated refactors.
- **Review pass:** independently inspect the diff, test boundary cases, validate claims against actual behavior, and request fixes rather than silently rewriting another contributor's work.
- Contributors may alternate these roles. The user is the final decision-maker for architecture changes and branch merges.

## Engineering rules

- Be explicit about the distinction between class-file analysis, executing isolated Java methods, translating a Java mod, and loading a complete Fabric mod. These are separate milestones.
- Never label a mod loadable based only on opcode similarity. Report a conservative porting verdict and its reasons.
- Do not silently ignore unknown bytecode, annotations, attributes, Mixin directives, access wideners, or Java APIs.
- All parsing and execution of untrusted mod content must be bounded. The Java subset interpreter must reject unsupported semantics, meter execution, and never provide ambient filesystem/network/process access.
- Add regression tests for every parser/interpreter bug. Prefer small real `javac` fixtures alongside synthetic malformed-input tests.
- Preserve iOS/ARM64 compatibility and avoid introducing a full JVM or JIT as an assumption.
