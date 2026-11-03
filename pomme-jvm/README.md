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

The current Fabric-JAR adapter supports a narrow proof-of-execution target:

- A valid `fabric.mod.json` with a `main` entrypoint class.
- Java class-file versions 45 through 65 (Java 21).
- No instance fields or static class initializer.
- A public no-argument constructor whose bytecode only calls
  `java.lang.Object.<init>()`.
- A public instance `onInitialize()V` method whose body is exactly `return`.

Example Java source:

```java
package example;

public final class ExampleMod {
    public ExampleMod() {}

    public void onInitialize() {}
}
```

Build a test JAR with a JDK 21:

```sh
mkdir -p build/classes
javac --release 21 -d build/classes src/example/ExampleMod.java
cat > build/classes/fabric.mod.json <<'JSON'
{
  "schemaVersion": 1,
  "id": "tiny_example",
  "version": "1.0.0",
  "entrypoints": {
    "main": ["example.ExampleMod"]
  }
}
JSON
jar --create --file tiny-example.jar -C build/classes .
cargo run -p pomme-jvm -- tiny-example.jar
```

A successful run reports the mod id, initialized class, and executed JVM
instruction count.

## Current limitations

This is **not general Fabric Loader compatibility** and does not yet run
ordinary third-party Fabric mods. It does not currently implement the Java
object heap, general constructor execution, field access, Java method calls,
exceptions, class initialization, class-loader delegation, Mixin, access
wideners, Fabric API, threads, garbage collection, or the Java standard
library. The narrow adapter validates a trivial constructor and executes the
empty instance initializer; it does not pretend that unsupported bytecode ran.

The next steps are a typed operand stack and object references, constructor
and method invocation, fields, exception tables, class loading/linking, and
then Fabric Loader's entrypoint and event APIs.
