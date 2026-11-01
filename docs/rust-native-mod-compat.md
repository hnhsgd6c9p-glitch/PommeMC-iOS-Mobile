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
  linear-memory cap for a minimal no-import WebAssembly module;
- parses Java class-file constant pools and method Code attributes, walks
  bytecode instruction boundaries, resolves invocation references, and
  aggregates per-JAR opcode/API compatibility inventories.

The restricted integer interpreter executes only a narrow subset of Java
bytecode. It does **not** implement a JVM, Fabric Loader, object semantics,
Java standard libraries, Mixins, general Java-to-WASM translation, or live game
integration. It cannot currently load normal Fabric mods.
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

### Stage 4 — Java class-file analysis (first implementation)

The `pomme-mod::classfile` module now contains a defensive class-file reader
and bytecode inventory. It reads the class header and constant pool, resolves
class/interface/method references, reads method descriptors and Code
attributes, walks bytecode instruction boundaries (including `tableswitch`,
`lookupswitch`, and `wide`), and records per-opcode counts. It also reports
class-file versions, annotations, static initializers, reflection/method-handle
usage, native methods, referenced classes, and high-risk Java API references.
`analyze_fabric_jar` scans the class entries of a JAR and aggregates the
inventory without extracting files. Malformed or oversized classes are
counted as skipped rather than executed.

A first restricted execution slice now exists in `pomme-mod::java_subset`:
`execute_static_i32` can run static Java methods with integer arguments and an
integer return using a bounded Rust interpreter. It supports integer constants,
local loads/stores, integer arithmetic/bitwise operations, comparisons, branches,
and returns. Each executed instruction consumes fuel, and stack/local bounds
are checked. Calls, heap/object operations, reflection, native methods, and
non-integer signatures are rejected. This is not a JVM and does not yet load
Fabric entrypoints or execute an entire mod.

The next translation boundary is now implemented experimentally in
`pomme-mod::ir`. `lower_static_i32_method` converts the supported bytecode
subset into typed Rust IR operations (typed integer arithmetic, local access,
conditions, and returns). Branches retain original bytecode offsets and are
validated to land on instruction boundaries. A control-flow graph builder
then splits methods into basic blocks and resolves branch targets to block
indexes. This provides the structural foundation for a Rust code generator.
This is still a lowering prototype only: it does not yet emit Rust source or machine code, perform full
JVM verifier stack analysis, or bridge Fabric APIs. Its accepted instruction
set is intentionally aligned with the restricted interpreter.

A `pomme-mod-inspect` command-line tool also prints a JAR's Fabric metadata,
class-file versions, bytecode opcode frequencies, API references, and detected
high-risk features. It is a triage tool for choosing realistic first porting
targets, not a mod loader.

Next, use real, permissively licensed Fabric mods as fixtures, expand the
interpreter to common compiler output (including constants, arrays, objects,
method dispatch, exceptions, and selected Java library calls), and build a
small Java-to-Pomme intermediate representation. Only a well-defined subset
should be translated to the Pomme mod ABI/WASM; unsupported instructions and
APIs must be explicit.

Treat Mixin-heavy mods, reflection-heavy mods, native methods, and mods that
require unavailable Java libraries as unsupported until their semantics are
implemented deliberately. Do not claim universal Java/Fabric compatibility.

### Stage 5 — compatibility catalogue

Test real mods by Minecraft version and dependency set. Publish explicit
compatibility results (native, translated subset, requires JVM, unsupported)
and never imply that all Fabric mods work.

## CI

The dedicated `Rust mod compatibility` workflow tests the package layer on
Linux, macOS, and Windows and checks that it compiles for `aarch64-apple-ios`.
It does not build or publish a Pomme IPA.
