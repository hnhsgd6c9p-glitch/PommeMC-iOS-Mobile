#!/usr/bin/env bash
# Xcode's build step: builds the client for the platform being built, with
# MoltenVK linked in, as the app's executable, and embeds OpenAL Soft, which
# the client loads at runtime. Xcode signs the app after this.
set -euo pipefail

case "$PLATFORM_NAME" in
    iphonesimulator) triple=aarch64-apple-ios-sim platform=ios-simulator ;;
    iphoneos) triple=aarch64-apple-ios platform=ios ;;
    *) echo "error: unsupported platform $PLATFORM_NAME" >&2; exit 1 ;;
esac
profile=debug
release=
if [ "$CONFIGURATION" = Release ]; then
    profile=release
    release=--release
fi

app="$TARGET_BUILD_DIR/$WRAPPER_NAME"
frameworks="$TARGET_BUILD_DIR/$FRAMEWORKS_FOLDER_PATH"
export PATH="$HOME/.cargo/bin:$PATH"
# Build scripts and native helper crates also compile host tools (including
# aws-lc-sys). Give those host compilations the macOS SDK explicitly; Xcode's
# inherited SDKROOT points at the iOS SDK and is not suitable for host C code.
export SDKROOT="$(xcrun --sdk macosx --show-sdk-path)"
unset LIBRARY_PATH

cd "$PROJECT_DIR/.."
# shaderc, built for the Mac, keeps using a real Vulkan SDK's copy when there
# is one; the app links MoltenVK as its Vulkan.
if [ -n "${VULKAN_SDK:-}" ]; then
    export SHADERC_LIB_DIR="$VULKAN_SDK/lib"
fi
VULKAN_SDK="$(python3 tools/fetch_moltenvk.py "$platform" | tail -n 1)"
export VULKAN_SDK
cargo build -p pomme-ios --target "$triple" ${release:+"$release"}
mkdir -p "$app" "$frameworks" "$app/JavaRuntime" "$app/FabricRuntime"
# Copied only when it changed, so an unchanged build isn't re-signed.
if ! cmp -s "target/$triple/$profile/pomme-ios" "$app/$EXECUTABLE_NAME"; then
    cp "target/$triple/$profile/pomme-ios" "$app/$EXECUTABLE_NAME"
fi

# Copy only the Java class-library module image: jlink output also contains
# host-specific native files that must not enter an iOS app bundle.
# JAVA_HOME discovery: prefer POMME_JAVA_HOME if it has bin/jlink,
# else JAVA_HOME if it has bin/jlink, else /usr/libexec/java_home -v 21.
# If none has bin/jlink and Resources/JavaRuntime/21 is incomplete, the
# existing error message names all three sources.
if [ -n "${POMME_JAVA_HOME:-}" ] && [ -x "${POMME_JAVA_HOME}/bin/jlink" ]; then
    JAVA_HOME_CANDIDATE="${POMME_JAVA_HOME}"
elif [ -n "${JAVA_HOME:-}" ] && [ -x "${JAVA_HOME}/bin/jlink" ]; then
    JAVA_HOME_CANDIDATE="${JAVA_HOME}"
else
    JAVA_HOME_CANDIDATE="$(/usr/libexec/java_home -v 21 2>/dev/null || true)"
fi
JAVA_RUNTIME_SOURCE="$PROJECT_DIR/Resources/JavaRuntime/21"
if [ -z "$JAVA_HOME_CANDIDATE" ] && { [ ! -f "$JAVA_RUNTIME_SOURCE/release" ] || [ ! -f "$JAVA_RUNTIME_SOURCE/lib/modules" ]; }; then
    echo "error: JavaRuntime/21 not found — set POMME_JAVA_HOME or JAVA_HOME, or install JDK 21 via /usr/libexec/java_home" >&2; exit 1
fi
if [ -z "$JAVA_HOME_CANDIDATE" ] && command -v /usr/libexec/java_home >/dev/null 2>&1; then
    candidate="$(/usr/libexec/java_home -v 21 2>/dev/null || true)"
    if [ -n "$candidate" ] && [ -x "$candidate/bin/jlink" ]; then
        JAVA_HOME_CANDIDATE="$candidate"
    fi
fi
mkdir -p "$app/JavaRuntime/21/lib"
JAVA_RUNTIME_PREEXISTS=1
if [ -f "$JAVA_RUNTIME_SOURCE/release" ] && [ -f "$JAVA_RUNTIME_SOURCE/lib/modules" ]; then
    JAVA_RUNTIME_PREEXISTS=1
else
    JAVA_RUNTIME_PREEXISTS=0
fi
if [ -f "$app/JavaRuntime/21/release" ] && [ -f "$app/JavaRuntime/21/lib/modules" ]; then
    # already staged from a previous successful build; keep as-is
    :
elif [ "$JAVA_RUNTIME_PREEXISTS" ]; then
    # pre-staged resources exist but were not yet copied; copy them now
    cp "$JAVA_RUNTIME_SOURCE/release" "$app/JavaRuntime/21/release"
    cp "$JAVA_RUNTIME_SOURCE/lib/modules" "$app/JavaRuntime/21/lib/modules"
elif [ -n "$JAVA_HOME_CANDIDATE" ]; then
    TEMP_JAVA_RUNTIME="${TMPDIR:-/tmp}/pomme-jlink-runtime-$$"
    "$JAVA_HOME_CANDIDATE/bin/jlink" --module-path "$JAVA_HOME_CANDIDATE/jmods" --add-modules ALL-MODULE-PATH \
        --strip-debug --no-man-pages --no-header-files --output "$TEMP_JAVA_RUNTIME"
    cp "$TEMP_JAVA_RUNTIME/release" "$app/JavaRuntime/21/release"
    cp "$TEMP_JAVA_RUNTIME/lib/modules" "$app/JavaRuntime/21/lib/modules"
else
    echo "error: JavaRuntime/21 not found — set POMME_JAVA_HOME or JAVA_HOME, or install JDK 21 via /usr/libexec/java_home, and ensure $JAVA_RUNTIME_SOURCE exists" >&2; exit 1
fi
[ -s "$app/JavaRuntime/21/release" ] && [ -s "$app/JavaRuntime/21/lib/modules" ] || { echo "error: incomplete JavaRuntime/21" >&2; exit 1; }
FABRIC_RESOURCE="$PROJECT_DIR/Resources/FabricRuntime/1.21.1"
[ -f "$FABRIC_RESOURCE/runtime.json" ] || { echo "error: missing Fabric runtime manifest" >&2; exit 1; }
# Fail fast: verify manifest fields before running bridge generator
python3 - "$FABRIC_RESOURCE" <<'PYVALIDATE_EARLY'
import json, pathlib, sys
root = pathlib.Path(sys.argv[1]); manifest = json.loads((root / "runtime.json").read_text())
if manifest.get("minecraftVersion") != "1.21.1" or manifest.get("javaMajor") != 21:
    raise SystemExit("error: Fabric runtime version/Java mismatch")
PYVALIDATE_EARLY
cp -R "$PROJECT_DIR/Resources/FabricRuntime/." "$app/FabricRuntime/"
python3 "$PROJECT_DIR/../pomme-jvm/scripts/build_minecraft_bridge.py" \
    "$app/FabricRuntime/1.21.1/builtin-mods/pomme-fabric-api-shim.jar" \
    "$app/FabricRuntime/1.21.1/builtin-mods/pomme-minecraft-api-bridge.jar"
# Verify bridge JARs are non-empty after generation
for jar in "$app/FabricRuntime/1.21.1/builtin-mods/pomme-fabric-api-shim.jar" "$app/FabricRuntime/1.21.1/builtin-mods/pomme-minecraft-api-bridge.jar"; do
    if [ ! -s "$jar" ]; then
        echo "error: bridge JAR is empty after generation: $jar" >&2
        exit 1
    fi
done
python3 - "$app/FabricRuntime/1.21.1" <<'PYVALIDATE'
import json, pathlib, sys
root = pathlib.Path(sys.argv[1]); manifest = json.loads((root / "runtime.json").read_text())
if manifest.get("minecraftVersion") != "1.21.1" or manifest.get("javaMajor") != 21:
    raise SystemExit("error: Fabric runtime version/Java mismatch")
required = [manifest["loaderJar"], manifest["intermediaryJar"], manifest["mixinJar"], manifest["providerJar"], *manifest["minecraftLibraries"]]
missing = [str(root / "lib" / n) for n in required if not (root / "lib" / n).is_file()]
missing += [str(root / "builtin-mods" / n) for n in manifest["builtinMods"] if not (root / "builtin-mods" / n).is_file()]
missing += [str(root / "builtin-mods" / n) for n in ("pomme-fabric-api-shim.jar", "pomme-minecraft-api-bridge.jar") if not (root / "builtin-mods" / n).is_file()]
if missing: raise SystemExit("error: missing runtime assets: " + ", ".join(missing))
PYVALIDATE
# The app bundle ships only the Java class-library image (release + lib/modules);
# any Mach-O in these trees is a packaging error.  The staged Resources/FabricRuntime
# and Resources/JavaRuntime trees may contain host-x86_64 dylibs from the build JDK;
# the find below only fails on files that made it into the app bundle.
while IFS= read -r -d '' binary; do
    description="$(file -b "$binary")"
    case "$description" in
      *Mach-O*)
        architectures="$(lipo -archs "$binary" 2>/dev/null || true)"
        echo "error: native Mach-O runtime file is forbidden in the Java/Fabric resources: $binary ($architectures)" >&2
        exit 1
        ;;
    esac
done < <(find "$app/JavaRuntime" "$app/FabricRuntime" -type f -print0)

python3 tools/build_openal.py "$platform" "$frameworks"
if [ -n "${EXPANDED_CODE_SIGN_IDENTITY:-}" ]; then
    codesign --force --sign "$EXPANDED_CODE_SIGN_IDENTITY" --timestamp=none \
        "$frameworks/soft_oal.framework"
fi
