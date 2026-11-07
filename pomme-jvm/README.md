# Pomme JVM

This crate is Pomme's Rust-implemented Java execution runtime. It currently
combines a fuel-bounded integer bytecode interpreter, defensive class-file
analysis, and an experimental Fabric JAR bootstrap path.

## Build and test

From the repository root:

```sh
cargo test -p pomme-jvm -p pomme-mod
cargo build -p pomme-jvm
```

The integration tests compile Java fixtures with `javac --release 8`, package
them into Fabric JARs, and exercise class loading and execution. If `javac`
is unavailable, those compiler-backed tests are skipped.

## Small Java Fabric example

The repo includes a minimal Java mod that exercises the JAR-to-entrypoint path
and the Pomme logging bridge:

```sh
cd pomme-jvm/examples/tiny-fabric-mod
sh build.sh
cd ../../../
cargo run -p pomme-jvm -- pomme-jvm/examples/tiny-fabric-mod/build/tiny-example.jar
```

The entrypoint implements Fabric's `net.fabricmc.api.ModInitializer` and is
discovered from the `main` entry in `fabric.mod.json`. Compile-time stubs for
Fabric's interface and `pomme.api.Pomme` are not packaged in the JAR.

## Implemented execution tier

The current runner supports a deliberately small Java 21-or-earlier subset:

- Fabric schema version 1 metadata and Java `main` entrypoints.
- Class files with major versions 45 through 65.
- `preLaunch`, `main`, and `client` entrypoint groups with their standard
  Fabric initializer interfaces and method names; `server` entrypoints are not
  run in the iOS client. String and object entrypoint descriptors are parsed,
  and `default` adapters plus per-entrypoint client/server environments are
  recognized.
- A validated public no-argument constructor that only invokes
  `java.lang.Object.<init>()`.
- String constants, reference local loads/stores, and `return`.
- Static helper calls whose parameters are `String` values and whose return
  type is `void`.
- Static pure-integer helper calls with `int` parameters and an `int` return,
  executed by the same class-path interpreter under the entrypoint's shared
  fuel budget.
- Static `int` field reads and writes (`getstatic`/`putstatic`) across the
  main JAR, nested JARs, and dependency class path.
- Bounded object allocation (`new`), reference duplication/storage, and
  no-argument constructors for classes on the loaded mod class path.
- Per-object `int` instance fields (`getfield`/`putfield`) and instance methods
  with typed `int`, `String`, and object-reference arguments and supported
  `void`, `int`, `String`, and reference returns. Includes parameterized
  constructors, superclass-aware lookup, virtual dispatch, and basic
  `invokeinterface` dispatch for class implementations.
- Java 9+ `StringConcatFactory` `invokedynamic` concatenation with `String` and
  integer arguments, including constant/recipe placeholders.
- One-time class initialization on entrypoint activation and before static
  method/field use; the current initializer subset supports bounded integer
  constants and static integer field writes.
- The explicit `pomme.api.Pomme.log(String)` and
  `pomme.api.Pomme.logInt(int)` native bridges.
- Class lookup across the main mod JAR, Fabric-declared nested JARs, and
  already-loaded dependency mod JARs.
- Fuel limits, call-depth limits, archive/class byte limits, duplicate-class
  rejection, and nested-JAR path validation.

The client keeps Java mods isolated by Minecraft version under
`<game-directory>/mods/<minecraft-version>/`. It scans Fabric metadata,
checks Minecraft/Java constraints, detects missing or incompatible required
mods, resolves dependency order, indexes library mods without main entrypoints,
and passes loaded dependency JARs into the runtime class path. The
`fabricloader` dependency is recognized as a platform service, but this is
not a version-identical Fabric Loader implementation.

## Important limitations

This is **not general JVM or Fabric Loader compatibility** and does not yet
run ordinary third-party Fabric mods. The interpreter does not yet implement
a complete Java object model, arrays, object fields other than `int`,
unsupported bytecode, primitive types beyond the supported integer-like subset, other `invokedynamic` bootstraps (lambdas, method references,
constant dynamics), Java exceptions, full class initialization ordering,
verification/linking, garbage collection, threads, or the Java standard
library.

Fabric services still missing include Fabric API events and registries,
Mixin transformation, access wideners, entrypoint adapters beyond simple class
names, client/server entrypoint lifecycles, Minecraft game-class bindings, and
full dependency/class-loader semantics. Modern Java features such as
`invokedynamic`, lambdas, method handles, and dynamic constants are not
executed. Unsupported features are reported rather than silently treated as
loaded.

The next JVM milestones are arrays, exceptions, verifier/linker semantics, broader Java library coverage,
and more complete class-loader behavior. The next Fabric milestones are API/event
bridges, Mixin/access-widener support, and compatibility tests against real
third-party mods.
