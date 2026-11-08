#!/bin/sh
set -eu

# Fetch the official Fabric Loader runtime for the selected Minecraft line.
# This is a build-time operation; Pomme never downloads Java dependencies on-device.
MC_VERSION="${1:-1.21.1}"
LOADER_VERSION="${2:-0.18.4}"
DEST="${3:-pomme-ios/Resources/FabricRuntime/$MC_VERSION}"
MAVEN="https://maven.fabricmc.net"
ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)"
JAVA_HOME="${JAVA_HOME:?Set JAVA_HOME to the bundled-runtime build JDK}"
export JAVA_HOME

mkdir -p "$DEST/lib"
fetch() {
    url="$1"
    output="$2"
    if [ ! -s "$output" ]; then
        curl --fail --location --silent --show-error "$url" -o "$output.tmp"
        mv "$output.tmp" "$output"
    fi
}

SOURCE_LOADER="/tmp/pomme-fabric-loader-$LOADER_VERSION.jar"
fetch "$MAVEN/net/fabricmc/fabric-loader/$LOADER_VERSION/fabric-loader-$LOADER_VERSION.jar" \
    "$SOURCE_LOADER"

# Ristretto's NIO ZIP filesystem cannot yet extract Fabric Loader's nested
# MixinExtras JAR at runtime. Extract declared nested libraries at build time
# and repackage the same official Loader metadata without its nested-JAR list.
mkdir -p "$DEST/builtin-mods"
python3 - "$SOURCE_LOADER" \
    "$DEST/lib/fabric-loader-$LOADER_VERSION-pomme.jar" "$DEST/builtin-mods" <<'PY'
import json
import os
import sys
import zipfile

source, destination, library_dir = sys.argv[1:]
with zipfile.ZipFile(source) as original:
    metadata = json.loads(original.read("fabric.mod.json"))
    nested = metadata.pop("jars", [])
    for entry in nested:
        relative = entry["file"]
        if not relative.startswith("META-INF/jars/") or ".." in relative.split("/") or "\\" in relative or not relative.endswith(".jar"):
            raise SystemExit("Unsafe nested Fabric Loader dependency path: " + relative)
        output = os.path.join(library_dir, os.path.basename(relative))
        with open(output, "wb") as stream:
            stream.write(original.read(relative))
    with zipfile.ZipFile(destination, "w", zipfile.ZIP_DEFLATED) as patched:
        for item in original.infolist():
            if item.filename == "fabric.mod.json" or item.filename.startswith("META-INF/jars/"):
                continue
            if item.filename.upper().endswith((".SF", ".RSA", ".DSA")):
                continue
            patched.writestr(item, original.read(item.filename))
        patched.writestr("fabric.mod.json", json.dumps(metadata, indent=2))
PY

fetch "$MAVEN/net/fabricmc/intermediary/$MC_VERSION/intermediary-$MC_VERSION-v2.jar" \
    "$DEST/lib/intermediary-$MC_VERSION-v2.jar"
fetch "$MAVEN/net/fabricmc/sponge-mixin/0.17.0+mixin.0.8.7/sponge-mixin-0.17.0+mixin.0.8.7.jar" \
    "$DEST/lib/sponge-mixin-0.17.0+mixin.0.8.7.jar"

for artifact in asm asm-analysis asm-commons asm-tree asm-util; do
    fetch "$MAVEN/org/ow2/asm/$artifact/9.9/$artifact-9.9.jar" \
        "$DEST/lib/$artifact-9.9.jar"
done

# Stage the Java-side libraries used by this Minecraft version. Native
# classifier JARs are deliberately excluded; Pomme owns rendering/audio natives.
python3 "$ROOT/pomme-jvm/scripts/fetch-minecraft-libraries.py" "$MC_VERSION" "$DEST/lib"

# Log4j 2.22 probes the removed sun.reflect.Reflection VM API on Java 21.
# Use a caller-class fallback that works in Ristretto's interpreter runtime.
PATCHER_CLASSES="/tmp/pomme-log4j-patcher"
mkdir -p "$PATCHER_CLASSES"
"$JAVA_HOME/bin/javac" -proc:none --release 21 \
    -cp "$DEST/lib/asm-9.9.jar:$DEST/lib/asm-tree-9.9.jar" \
    -d "$PATCHER_CLASSES" "$ROOT/pomme-jvm/tools/PatchLog4jStackLocator.java"
"$JAVA_HOME/bin/java" \
    -cp "$PATCHER_CLASSES:$DEST/lib/asm-9.9.jar:$DEST/lib/asm-tree-9.9.jar" \
    PatchLog4jStackLocator "$DEST/lib/log4j-api-2.22.1.jar"

# Compile the initial Fabric API compatibility surface. This provides the
# real Fabric Event ABI and client tick events while the broader Minecraft API
# bridge is built out; it intentionally does not claim the whole Fabric API.
FABRIC_API_VERSION="0.116.9+1.21.1-pomme.1"
mkdir -p "$DEST/builtin-mods"
API_CLASSES="/tmp/pomme-fabric-api-shim-classes-$MC_VERSION"
mkdir -p "$API_CLASSES"
find "$ROOT/pomme-jvm/resources/fabric-api-shim/src" -name '*.java' ! -path '*/sun/reflect/*' -print > "$API_CLASSES/sources.list"
"$JAVA_HOME/bin/javac" -proc:none --release 21 -cp "$DEST/lib/*" -d "$API_CLASSES" @"$API_CLASSES/sources.list"
cp "$ROOT/pomme-jvm/resources/fabric-api-shim/fabric.mod.json" "$API_CLASSES/fabric.mod.json"
"$JAVA_HOME/bin/jar" --create --file "$DEST/builtin-mods/pomme-fabric-api-shim.jar" -C "$API_CLASSES" .

fetch "https://repo.maven.apache.org/maven2/org/slf4j/slf4j-api/2.0.17/slf4j-api-2.0.17.jar" \
    "$DEST/lib/slf4j-api-2.0.17.jar"
fetch "https://repo.maven.apache.org/maven2/org/slf4j/slf4j-simple/2.0.17/slf4j-simple-2.0.17.jar" \
    "$DEST/lib/slf4j-simple-2.0.17.jar"

# Download official Fabric API modules for ABI-only fixture compilation. The
# runtime still ships Pomme's compatible bridge JAR, not Mojang/Fabric code.
OFFICIAL_FABRIC_API_VERSION="0.116.9+1.21.1"
OFFICIAL_FABRIC_API="/tmp/pomme-official-fabric-api-$MC_VERSION.jar"
fetch "$MAVEN/net/fabricmc/fabric-api/fabric-api/$OFFICIAL_FABRIC_API_VERSION/fabric-api-$OFFICIAL_FABRIC_API_VERSION.jar" \
    "$OFFICIAL_FABRIC_API"
python3 - "$OFFICIAL_FABRIC_API" "$MC_VERSION" <<'PY'
import os
import sys
import zipfile

archive_path, mc_version = sys.argv[1:]
destination = f"/tmp/pomme-fabric-api-abi-{mc_version}"
os.makedirs(destination, exist_ok=True)
found = {}
all_modules = os.path.join(destination, "all-modules")
os.makedirs(all_modules, exist_ok=True)
with zipfile.ZipFile(archive_path) as archive:
    for name in archive.namelist():
        base = name.rsplit("/", 1)[-1]
        if name.startswith("META-INF/jars/") and base.endswith(".jar"):
            with open(os.path.join(all_modules, base), "wb") as output:
                output.write(archive.read(name))
        if base.startswith("fabric-api-base-") and base.endswith(".jar"):
            found["base"] = archive.read(name)
        elif base.startswith("fabric-lifecycle-events-v1-") and base.endswith(".jar"):
            found["lifecycle"] = archive.read(name)
if set(found) != {"base", "lifecycle"}:
    raise SystemExit("Could not find official Fabric API base/lifecycle ABI fixtures")
paths = []
for key in ("lifecycle", "base"):
    path = os.path.join(destination, f"fabric-{key}.jar")
    with open(path, "wb") as output:
        output.write(found[key])
    paths.append(path)
with open(f"/tmp/pomme-fabric-api-test-classpath-{mc_version}", "w") as output:
    output.write(":".join(paths))
PY

# Compile Pomme's GameProvider against the exact pinned Fabric Loader API.
CLASSES="/tmp/pomme-fabric-provider-classes-$MC_VERSION"
mkdir -p "$CLASSES"
"$JAVA_HOME/bin/javac" -proc:none --release 21 \
    -cp "$DEST/lib/fabric-loader-$LOADER_VERSION-pomme.jar:$DEST/lib/sponge-mixin-0.17.0+mixin.0.8.7.jar:$DEST/lib/asm-9.9.jar:$DEST/lib/asm-tree-9.9.jar:$DEST/lib/asm-commons-9.9.jar:$DEST/lib/asm-util-9.9.jar:$DEST/builtin-mods/pomme-fabric-api-shim.jar" \
    -d "$CLASSES" \
    "$ROOT/pomme-jvm/resources/fabric/PommeGameProvider.java" \
    "$ROOT/pomme-jvm/resources/fabric/PommeGameEntrypoint.java" \
    "$ROOT/pomme-jvm/resources/fabric/PommeFabricLauncher.java"
mkdir -p "$CLASSES/META-INF/services"
cp "$ROOT/pomme-jvm/resources/fabric/META-INF/services/net.fabricmc.loader.impl.game.GameProvider" \
    "$CLASSES/META-INF/services/"
"$JAVA_HOME/bin/jar" --create --file "$DEST/lib/pomme-fabric-provider.jar" -C "$CLASSES" .

# Generate the versioned Minecraft ABI from the official client metadata and
# Fabric intermediary mappings. Only inert class signatures are retained;
# the original game bytecode is never packaged into the IPA.
"$ROOT/pomme-jvm/scripts/generate-minecraft-api-stubs.sh" "$MC_VERSION"
"$ROOT/pomme-jvm/scripts/verify-minecraft-api-stubs.sh" "$MC_VERSION"

# Audit every nested official Fabric API module. Base/lifecycle must have no
# missing ABI symbols; the full API inventory may expose Mixin-added methods
# which remain the next compatibility milestone.
ABI_DIR="/tmp/pomme-fabric-api-abi-$MC_VERSION"
REPORT_DIR="$ROOT/pomme-jvm/reports/$MC_VERSION"
mkdir -p "$REPORT_DIR"
ALL_REPORT="$REPORT_DIR/fabric-api-all-modules-abi.json"
BASE_REPORT="$REPORT_DIR/fabric-api-base-lifecycle-abi.json"
set -- "$ABI_DIR"/all-modules/*.jar
POMME_ABI_REPORT="$ALL_REPORT" "$ROOT/pomme-jvm/scripts/audit-mod-abi.sh" "$@" > "/tmp/pomme-fabric-api-all-audit-$MC_VERSION.log"
POMME_ABI_REPORT="$BASE_REPORT" "$ROOT/pomme-jvm/scripts/audit-mod-abi.sh" \
    "$ABI_DIR/fabric-base.jar" "$ABI_DIR/fabric-lifecycle.jar" > "/tmp/pomme-fabric-api-base-audit-$MC_VERSION.log"
python3 - "$ALL_REPORT" "$BASE_REPORT" <<'PY'
import json
import sys
all_modules = json.load(open(sys.argv[1]))
base_lifecycle = json.load(open(sys.argv[2]))
if base_lifecycle["missingClassCount"] or base_lifecycle["missingMemberCount"]:
    raise SystemExit("Fabric API base/lifecycle ABI audit failed")
if all_modules["missingClassCount"]:
    raise SystemExit("Full Fabric API audit found missing Minecraft classes")
print(
    "Fabric API ABI audit: base/lifecycle complete; all modules reference "
    f"{all_modules['referencedMinecraftClassCount']} Minecraft classes with "
    f"{all_modules['missingMemberCount']} Mixin-provided extension members still pending"
)
PY
: # Keep the small temporary compiler output for reproducible diagnostics

python3 - "$MC_VERSION" "$LOADER_VERSION" "$DEST" <<'PY'
import json
import os
import sys

mc_version, loader_version, destination = sys.argv[1:]
with open(os.path.join(destination, "minecraft-libraries.json")) as libraries_file:
    minecraft_libraries = [item["file"] for item in json.load(libraries_file)]
manifest = {
    "minecraftVersion": mc_version,
    "javaMajor": 21,
    "loaderJar": f"fabric-loader-{loader_version}-pomme.jar",
    "intermediaryJar": f"intermediary-{mc_version}-v2.jar",
    "mixinJar": "sponge-mixin-0.17.0+mixin.0.8.7.jar",
    "providerJar": "pomme-fabric-provider.jar",
    "minecraftLibraries": minecraft_libraries,
    "builtinMods": [
        "pomme-fabric-api-shim.jar",
        "mixinextras-fabric-0.5.0.jar"
    ]
}
with open(os.path.join(destination, "runtime.json"), "w") as output:
    json.dump(manifest, output, indent=2)
    output.write("\n")
PY

printf 'Fabric Loader %s runtime staged for Minecraft %s in %s\n' \
    "$LOADER_VERSION" "$MC_VERSION" "$DEST"
