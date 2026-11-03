# Pomme JVM

This crate is the beginning of Pomme's Rust-implemented JVM. It currently
contains a fuel-bounded interpreter for a limited integer-bytecode subset and
an experimental runner for a deliberately tiny Fabric Java mod.

## Build

From the repository root:

```sh
cargo test -p pomme-jvm -p pomme-mod
cargo build -p pomme-jvm
```

## First supported Java mod

The repo includes a small Java example that proves the full JAR-to-entrypoint
path and calls a native Pomme API bridge:

```sh
cd pomme-jvm/examples/tiny-fabric-mod
sh build.sh
cd ../../../
cargo run -p pomme-jvm -- pomme-jvm/examples/tiny-fabric-mod/build/tiny-example.jar
```

Expected output includes:

```text
Loaded Java Fabric mod: tiny_example
  initialized example.ExampleMod
[Java mod] Hello from a Java Fabric mod!
Executed 3 JVM instruction(s).
```

The Java class implements Fabric's `net.fabricmc.api.ModInitializer` interface
and is discovered through the standard `main` entrypoint metadata. Compile-time
stubs for Fabric's interface and `pomme.api.Pomme` are not packaged in the JAR.
The Java source calls `pomme.api.Pomme.log(String)`. The Rust runtime
resolves that method reference and handles the call through its native bridge.

The supported Java class shape is intentionally narrow:

- A valid `fabric.mod.json` with a `main` entrypoint class.
- Java class-file versions 45 through 65 (Java 21).
- No instance fields or static class initializer.
- A public no-argument constructor whose bytecode only calls
  `java.lang.Object.<init>()`.
- A public instance `onInitialize()V` method containing string constants,
  calls to `pomme.api.Pomme.log(String)`, and a final `return`.

## Current limitations

This is **not general Fabric Loader compatibility** and does not yet run
ordinary third-party Fabric mods. It does not currently implement the Java
object heap, general constructor execution, field access, Java method calls,
exceptions, class initialization, class-loader delegation, Mixin, access
wideners, Fabric API, threads, garbage collection, or the Java standard
library. The narrow adapter validates a trivial constructor and executes only a small
instruction subset. It supports string constants and one native bridge call,
`pomme.api.Pomme.log(String)`; unsupported bytecode is rejected rather than
silently skipped.

The next steps are a typed operand stack and object references, constructor
and method invocation, fields, exception tables, class loading/linking, and
then Fabric Loader's entrypoint and event APIs.
