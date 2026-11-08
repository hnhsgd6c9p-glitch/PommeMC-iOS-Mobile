# Pomme JVM and Fabric runtime

Pomme runs Java bytecode through Ristretto 0.35.0 and embeds the official Fabric Loader/Knot runtime for the Minecraft 1.21.1 compatibility profile. The Rust client remains responsible for rendering and native game execution; Fabric mods execute in a retained Java VM connected to the client through Pomme's game-provider and API bridge.

## Runtime layers

- **Ristretto 0.35.0:** interpreter-mode JVM suitable for iOS app sandbox constraints; no executable JIT memory is assumed.
- **Java 21 / Java 25:** runtime-image selection is versioned by Minecraft line. Java 21 is used for 1.21.x and Java 25 for 26.x.
- **Fabric Loader 0.18.4 / Knot:** official loader discovery, metadata validation, dependency solving, entrypoint phases, language-adapter infrastructure, and class-loader setup for Minecraft 1.21.1.
- **SpongePowered Mixin 0.8.7 + ASM 9.9:** pinned and staged. Ristretto 0.35 implements Java-defined class loading through `ClassLoader.defineClass1/2`, `findLoadedClass0` and related intrinsics. The Fabric Loader/MixinExtras probe has passed and verified that a transformed class reaches Ristretto's defineClass path. Broad real-Minecraft target coverage remains in progress.
- **MixinExtras 0.5.0:** extracted at build time from Fabric Loader's nested dependency and staged as a standalone mod because Ristretto's ZIP filesystem cannot yet perform Fabric's runtime nested-JAR extraction.
- **Intermediary 1.21.1 mappings:** exposed as a built-in mod and used as Pomme's initial runtime namespace.
- **Pomme GameProvider:** supplies the game directory, mods directory, Minecraft version, built-in Minecraft/intermediary metadata, and a Rust-client entrypoint.
- **Generated Minecraft ABI:** a build-time Tiny Remapper + ASM pipeline reads the official 1.21.1 client class metadata, remaps it to intermediary names, and generates 8,269 inert Minecraft/Mojang class stubs. The sixteen hand-written bridge classes are merged with generated members while retaining their Rust-backed behavior. Original Minecraft method bodies are not copied into the generated JAR.
- **Minecraft Java dependencies:** 44 Java-side dependency JARs (57.7 MiB) are staged from the official version manifest; native classifier JARs are excluded. Some libraries still contain native-dependent APIs that need platform-specific bridges.
- **Mobile Log4j compatibility:** the build patches Log4j 2.22.1's legacy caller-class fallback so Java 21/Ristretto can create loggers without the removed `sun.reflect.Reflection` VM hook. Desktop-only JMX is disabled in the mobile launch environment.

The Fabric runtime is built by `pomme-jvm/scripts/fetch-fabric-runtime.sh` and packaged under `FabricRuntime/<minecraft-version>/`. The Java module images are packaged separately under `JavaRuntime/<major>/`. No JDK is downloaded on the iPhone.

## Live Fabric integration

The client scans the selected `mods/<minecraft-version>/` directory, validates package metadata and archive limits, then starts the official Fabric Loader in a persistent Ristretto VM. Fabric Loader owns mod discovery and dependency resolution; Pomme no longer manually invokes the three initializer interfaces as a substitute for the loader.

The retained JVM session exposes callback dispatch into the Knot target class loader. Pomme's fixed-step client loop dispatches Fabric `ClientTickEvents.START_CLIENT_TICK` and `END_CLIENT_TICK` callbacks. The merged compatibility JAR contains the Fabric Event ABI, client tick events, sixteen hand-written bridge classes, and generated intermediary Minecraft/Mojang class signatures. The generated methods currently return inert defaults; they provide broad linkage and a complete class/member inventory, not functioning Minecraft behavior. Actual game-facing behavior still requires explicit Rust bridge implementations.

## Integration test coverage

The Fabric Loader bootstrap test has passed on Java 21/Ristretto. It compiles a Java Fabric mod against official Fabric API base/lifecycle modules, runs it against Pomme's ABI bridge, dispatches client/world start/end tick callbacks, reads player position and health, and verifies a MixinExtras `@WrapMethod` transformation before class definition.

A real third-party AppleSkin 3.0.6 fixture was then loaded with the official Fabric API. Fabric Loader discovers it and its common initializer reaches networking registration. After fixing the payload codec, identifier factory, JMX, and Log4j caller-class blockers, the client-side launch path still becomes CPU-bound while loading configuration dependencies (likely Cloth Config/Gson); the exact Ristretto frame still needs isolation. That test is retained as an ignored manual regression target rather than allowed to stall the normal suite.

The next automated third-party target is Lithium 0.15.4 for Minecraft 1.21.1. Its dedicated test checks Fabric Loader discovery, actual `ModInitializer` execution, Mixin class loading, and a transformed Minecraft target. The test runs in the iOS IPA workflow after the pinned fixture and runtime are staged.

None of these tests proves that every arbitrary mod works, that all Minecraft APIs are functional, or that the iOS IPA has been installed and tested on an iPhone.

## Current Minecraft and Fabric API surface

Implemented so far:
- Fabric Loader game-provider integration and versioned runtime staging for Minecraft 1.21.1.
- Fabric's pre-launch, main, and client initializer lifecycle.
- Fabric default entrypoint language adapter behavior, including class factory descriptors.
- Nested MixinExtras packaging workaround.
- Mixin configuration packaging and bootstrap; the probe verifies transformation-before-definition for its target class, while broad vanilla/Minecraft target coverage remains unverified.
- Fabric Event registration and client start/end tick events.
- An intermediary Minecraft client/player/entity surface (`class_310`, `class_746`, `class_1657`, `class_1309`, `class_1297`, `class_1702`) exposing player position, health, food, ground state, game directory and version through a per-tick Rust-to-Java state snapshot.
- Generated Minecraft 1.21.1 intermediary ABI stubs for 8,269 Minecraft/Mojang classes, with matching fields, method descriptors, access flags and class hierarchy. The merged bridge currently contains 8,278 classes, including hand-written overrides.
- An ASM-based mod ABI audit tool; official Fabric API base/lifecycle fixtures report zero missing classes and members against the generated bridge.
- Fabric Event ABI compatibility including client and world start/end tick events, plus intermediary `Identifier` support.
- Persistent VM callback dispatch from the native fixed-step loop.

Not implemented yet:
- Functional implementations for most generated Minecraft methods. The generated API stubs intentionally return defaults and must be connected to real Rust game state subsystem by subsystem.
- The complete functional Minecraft 1.21.1 registries, entities, inventory, world, networking, rendering, UI and resource APIs.
- The complete Fabric API module set (rendering, networking, key bindings, lifecycle, registry, resource, item, block, entity, screen and other APIs).
- A complete native bridge from Java API calls to Pomme's Rust world, player, renderer, input, audio and resource systems.
- Broad proof that Fabric Knot's transformer correctly applies every real mod's Mixin targets before Ristretto's Java `defineClass1/2` path. The synthetic MixinExtras probe passes; real third-party target coverage is still being expanded.
- Full Fabric API extension coverage: the all-module audit currently finds 75 methods/fields expected to be injected by Fabric API Mixins. These should be supplied by the official Mixin pipeline rather than hard-coded into the vanilla ABI generator.
- Broad Mixin target coverage against real Minecraft classes, including all injection points and compatibility edge cases.
- Full access-widener behavior and remapping validation across all supported mod namespace variants.
- Java 25 / Fabric Loader profile for Minecraft 26.x.
- Broad compatibility testing against real third-party mods, including AppleSkin's Cloth Config/Gson client initialization and large rendering mods. The Lithium 0.15.4 integration test is the next CI gate.
- iPhone device verification of the bundled JVM, Loader, Mixin transformations and real gameplay callbacks.

## General JVM backend

The Ristretto backend provides:
- Java class loading from directories and JARs.
- Java arrays, object allocation, reflection, static initialization, exception handling, lambdas, method references and standard-library execution.
- URL handling and Rustls/Ring TLS support.
- A persistent VM session for callbacks after Fabric initialization.
- Interpreter mode for iOS, avoiding assumptions about JIT memory permissions.

The original fuel-bounded subset interpreter remains for developer builds that do not contain an embedded Java runtime. It is not considered full Java compatibility.

## Build and test

From the repository root:

    cargo test -p pomme-jvm -p pomme-mod
    cargo test -p pomme-client --lib
    cargo check --target aarch64-apple-ios -p pomme-client --lib

Set `JAVA_HOME` to a full JDK 21 or JDK 25 installation for runtime and fixture tests. The iOS IPA workflow stages the Java runtime images and the pinned Fabric Loader/Mixin runtime before XcodeGen packages the app.

## Automated Minecraft ABI audit

Run the bytecode-level compatibility audit against one or more Fabric mod JARs:

    JAVA_HOME=/path/to/jdk-21 pomme-jvm/scripts/audit-mod-abi.sh path/to/mod.jar [another-mod.jar]

The scanner uses the bundled ASM 9.9 library to inspect class, field, method, descriptor, annotation, handle and invokedynamic references without executing or decompiling the mod. It compares intermediary Minecraft references against Pomme's currently bundled shim, including inherited members, and writes a JSON report to `pomme-abi-report.json` (override with `POMME_ABI_REPORT`).

Initial 1.21.1 baseline:
- The tiny example Fabric mod reports zero Minecraft ABI gaps.
- The generated bridge contains 8,278 classes; Java 21 reflection successfully resolves their class hierarchies, fields, methods and constructors with zero failures.
- Official Fabric API base/lifecycle modules have zero missing Minecraft classes and zero missing members.
- All 48 nested modules in Fabric API 0.116.9 reference 728 intermediary Minecraft classes. The generated bridge covers all 728 classes, but 75 Fabric API extension members remain absent because they are normally injected by Mixin. The report is stored at `pomme-jvm/reports/1.21.1/fabric-api-all-modules-abi.json`.

This inventory separates vanilla Minecraft ABI coverage from Fabric API's Mixin-injected extension surface. Zero missing classes does not mean those extension methods work; it identifies the next concrete transformation/API gap.

## Compatibility policy

Pomme will not label Java/Fabric support as complete based only on JVM language tests or a synthetic probe mod. Full compatibility requires the supported Minecraft version's Java/Fabric APIs to be implemented, the corresponding transformation pipeline to work against real game classes, and representative third-party mods to load and function on the iPhone.
