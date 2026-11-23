# Pomme engineering handoff

_Last updated: 2026-11-03_

## Worktree and branches

- The Java/Fabric compatibility experiment is on `experiment/rust-native-mod-compat` in `PommeMC-iOS-Mobile`.
- The iOS IPA work remains isolated on `ios-ipa-cloud-build` in `/Users/aiden/Pomme`.
- The shader/fast-mode optimization work is on `experiment/ios-umo-shader-fastmode` in `PommeMC-iOS-Mobile`.
- Keep these efforts isolated until the user chooses an integration target.

## Mod compatibility work completed

- Defensive Fabric JAR and Java class-file inspection, with archive entry, per-class, aggregate class-byte, and class-count limits.
- Fabric metadata inventory, opcode/method/field-reference summaries, and conservative compatibility verdicts.
- Restricted fuel-metered integer bytecode interpreter and typed IR/control-flow graph lowering.
- Dynamic-linkage inventory for `invokedynamic`, method handles and types, dynamic constants, and `BootstrapMethods`.
- `pomme-mod-inspect` reports those features; dynamic-linkage features classify as `HardJava`.
- Java 8 source fixtures cover integer arithmetic/control flow and lambda bootstraps. CI installs Temurin 21 to compile them with `javac --release 8`.

## Iris shaderpack port progress

- Pomme already has a native Iris/OptiFine-format pipeline: it reads folder or ZIP packs, preprocesses and translates GLSL to SPIR-V, then runs shadow, gbuffer, deferred/composite, and final passes through its Vulkan renderer (MoltenVK on iOS).
- This is shaderpack compatibility code, not the Java Iris mod or its Minecraft mixins. Pack support is still partial: clouds, particles, weather, hand/item, and glint draw paths are not connected to pack programs yet; some Iris uniforms also use defaults.
- `biome_category` now maps the active biome registry name to Iris's legacy category IDs instead of always returning `CAT_NONE`.
- Next shader milestone: route particle and weather geometry into Iris `gbuffers_*` programs, then handle held items/clouds and fill out the remaining uniforms. Track packs that compile and render correctly on iOS separately from desktop Vulkan.

## Mobile rendering and version selection

- iOS defaults now use a 16-chunk render distance and 16-chunk full-detail radius; farther loaded terrain meshes through progressively coarser LOD levels. The protocol can request at most 127 chunks because the client view-distance field is a signed byte, and the cache reserves 128 chunks.
- Particle and precipitation vertex scratch buffers are retained between frames. Water sections are sorted back-to-front by section center to reduce translucent overlap artifacts.
- Options can select a Pomme version from the translated 1.20.1–26.3 protocol list. The selection takes effect after restart and creates `profiles/<name>/mods/<version>` for that version.
- The current `pomme-mod` code only inspects Fabric packages and experiments with a tiny Java subset / WASM sandbox. The live client does not load Fabric Java mods or execute Pomme WASM mods yet. The 1,000-chunk horizon also needs terrain from a server extension or a persistent distant-terrain cache; the normal protocol does not provide that range.

## Validation status

- `cargo test -p pomme-mod --locked`: 30 Rust unit tests pass on this host.
- The two `javac` integration checks are present but skip locally because this Mac has no JDK. The package CI installs a JDK and runs them.
- Re-run `cargo fmt -p pomme-mod -- --check`, `cargo clippy -p pomme-mod --all-targets --locked -- -D warnings`, and `cargo check -p pomme-mod --target aarch64-apple-ios --locked` before merging.

## Limits that remain

- This is not a JVM, Fabric Loader, or end-to-end Java mod port.
- No Fabric lifecycle, event bus/API, Mixin, access widener, object model, arrays, exceptions, class loading, or general Java library support exists.
- `SimpleJavaCandidate` only says observed bytecode fits a tiny static integer subset. It does not mean a mod can load.
- `pomme.mod.json` and the WASM ABI remain experimental and are not connected to the live game loop.

## Next milestones

1. Run the JDK-backed fixture checks in CI (or install a local JDK) and fix any real compiler-output gaps.
2. Add permissively licensed real Fabric JAR fixtures and publish conservative per-version/dependency verdicts.
3. Define a versioned Pomme mod host ABI and connect the sandbox to game lifecycle hooks.
4. Benchmark WASM startup, fuel metering, and memory on an iPhone before enabling modules in the iOS app.
5. Keep Java class execution disabled until verification, class-library behavior, and explicit resource limits are sufficient.
