# Rust-native mod compatibility (experimental)

This experiment explores a mod ecosystem for Pomme without replacing its Rust
client with a JVM-based Minecraft client. It is intentionally isolated on the
`experiment/rust-native-mod-compat` branch.

## The compatibility boundary

A Fabric mod is not just a ZIP file with metadata. Fabric Loader reads
`fabric.mod.json`, resolves dependencies and entrypoints, loads Java classes,
and supports class transformation through Mixin. The Fabric documentation
makes clear that entrypoints instantiate Java code and Mixins transform game
and mod classes. Parsing the metadata is therefore only discovery, not running
the mod.

There are three different goals, and they should not be conflated:

1. **Fabric package discovery:** inspect a JAR, read its metadata, enumerate
   entrypoints/dependencies/nested JARs/Mixin declarations, and explain why it
   is or is not currently compatible.
2. **Rust-native Pomme mods:** provide a stable host API with a sandboxed
   WebAssembly module format. Rust-authored mods can compile to WASM without
   linking arbitrary native dylibs into the iOS app.
3. **Java/Fabric compatibility:** translate supported Java bytecode and
   selected Fabric APIs to the native mod ABI, or evaluate a portable JVM
   interpreter. This is a much larger project. Mixins, reflection, arbitrary
   Java libraries, Kotlin adapters, and mods coupled to Minecraft internals
   prevent a universal one-click conversion.

## Why not embed a normal JVM?

A normal desktop JVM commonly relies on JIT compilation and runtime class
loading. iOS imposes executable-memory and code-signing restrictions that make
that approach a poor foundation for an installable mobile app. A portable
interpreter is a more plausible route, but it still needs Java class libraries,
class verification, reflection, threading, GC, JNI substitutes, and Minecraft
API shims. It would also add memory and CPU overhead that works against the
purpose of Pomme's native Rust engine.

The first native extension target should therefore be WebAssembly with a
small host interface and explicit permissions. A pure interpreter such as
Wasmi is a candidate for iOS because it avoids a JIT, but it must be benchmarked
and reviewed before adoption. Fuel/instruction limits, memory caps, capability
checks, and a versioned ABI are required before executing untrusted mods.

## Current implementation

The `pomme-mod` crate currently:

- reads JAR/ZIP packages without extracting their contents;
- parses Fabric `fabric.mod.json` metadata and records dependencies,
  entrypoints, nested JARs, mixins, and access-widener declarations;
- identifies archives containing JVM `.class` files as requiring a Java
  compatibility layer rather than pretending they are native Rust mods;
- defines a separate `pomme.mod.json` manifest for a WebAssembly module;
- validates identifiers, manifest size, archive entry count, and relative
  module paths to avoid straightforward path-traversal and oversized-manifest
  mistakes;
- includes a Wasmi interpreter prototype with fuel metering and a 16 MiB
  linear-memory cap for a minimal no-import WebAssembly module.

It does **not** execute Java bytecode, implement Fabric Loader, apply Mixins,
translate Java classes, or connect the WebAssembly runtime to the live game.
The `pomme.mod.json` manifest and current WASM export contract are experimental
and not a stable public format.

## Planned stages

### Stage 1 — package inspection (current)

Build a cross-platform scanner and test malformed packages, dependencies,
nested JAR declarations, Mixin declarations, and path validation.

### Stage 2 — native mod ABI

Define host calls for lifecycle, event subscription, commands, registries,
resource access, and UI hooks. Keep game internals private behind versioned
handles. Define explicit capabilities instead of giving mods unrestricted
access to client state or filesystem paths.

### Stage 3 — WASM runtime (interpreter prototype exists)

The crate now includes a Wasmi 2.x interpreter prototype. It validates a
module, invokes a no-argument/no-result export, applies a 16 MiB linear-memory
cap, and meters execution with a caller-supplied fuel budget. It intentionally
provides no WASI imports, filesystem, networking, or Pomme host calls yet.
The interpreter is not connected to the live game loop and the experimental
manifest/ABI is not stable. Benchmark on the target iPhone before deciding
whether this interpreter profile is fast enough. Do not enable arbitrary native
dynamic libraries on iOS.

### Stage 4 — Java compatibility research

Build a class-file parser and bytecode feature inventory. Start with simple
mods that use a narrow subset of Java and Fabric entrypoints. Design an
intermediate representation and map only supported APIs to the native ABI.
Treat Mixin-heavy mods as unsupported until class transformation semantics
are deliberately implemented. Report unsupported instructions and API calls
rather than silently skipping them.

### Stage 5 — compatibility catalogue

Test real mods by Minecraft version and dependency set. Publish explicit
compatibility results (native, translated subset, requires JVM, unsupported)
and never imply that all Fabric mods work.

## CI

The dedicated `Rust mod compatibility` workflow tests the package layer on
Linux, macOS, and Windows and checks that it compiles for `aarch64-apple-ios`.
It does not build or publish a Pomme IPA.
