#!/bin/sh
set -eu
ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)"
if [ "$#" -ge 1 ]; then MC_VERSION="$1"; else MC_VERSION="1.21.1"; fi
DEST="$ROOT/pomme-ios/Resources/FabricRuntime/$MC_VERSION"
JAVA_HOME="$(printenv JAVA_HOME 2>/dev/null || true)"
if [ -z "$JAVA_HOME" ]; then echo "Set JAVA_HOME to JDK 21" >&2; exit 2; fi
JAVA="$JAVA_HOME/bin/java"
JAVAC="$JAVA_HOME/bin/javac"
ASM="$DEST/lib/asm-9.9.jar"
ASM_TREE="$DEST/lib/asm-tree-9.9.jar"
INTERMEDIARY="$DEST/lib/intermediary-$MC_VERSION-v2.jar"
SHIM="$DEST/builtin-mods/pomme-fabric-api-shim.jar"
JAR="$JAVA_HOME/bin/jar"
if [ ! -s "$ASM" ] || [ ! -s "$ASM_TREE" ] || [ ! -s "$INTERMEDIARY" ]; then
  echo "Fabric runtime, ASM and intermediary mappings must be staged first." >&2
  exit 2
fi
WORK="/tmp/pomme-minecraft-api-$MC_VERSION"
mkdir -p "$WORK"
BASE="$WORK/pomme-fabric-api-shim-base.jar"
BASE_CLASSES="$WORK/base-shim-classes"
mkdir -p "$BASE_CLASSES"
find "$ROOT/pomme-jvm/resources/fabric-api-shim/src" -name '*.java' ! -path '*/sun/reflect/*' -print > "$WORK/shim-sources.list"
"$JAVAC" -proc:none --release 21 -cp "$DEST/lib/*" -d "$BASE_CLASSES" @"$WORK/shim-sources.list"
cp "$ROOT/pomme-jvm/resources/fabric-api-shim/fabric.mod.json" "$BASE_CLASSES/fabric.mod.json"
"$JAR" --create --file "$BASE" -C "$BASE_CLASSES" .
python3 - "$MC_VERSION" "$WORK/client-official.jar" <<'PY'
import hashlib,json,os,sys,urllib.request
version,path=sys.argv[1:]
manifest=json.load(urllib.request.urlopen("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json"))
entry=next(v for v in manifest["versions"] if v["id"]==version)
meta=json.load(urllib.request.urlopen(entry["url"]))
client=meta["downloads"]["client"]
def sha1(file):
    digest=hashlib.sha1()
    with open(file,"rb") as handle:
        for chunk in iter(lambda:handle.read(1024*1024),b""): digest.update(chunk)
    return digest.hexdigest()
if not os.path.isfile(path) or sha1(path)!=client["sha1"]:
    temp=path+".tmp"
    print("Downloading and verifying Minecraft client metadata source for",version)
    urllib.request.urlretrieve(client["url"],temp)
    if sha1(temp)!=client["sha1"]:
        os.remove(temp)
        raise SystemExit("Minecraft client SHA-1 verification failed")
    os.replace(temp,path)
print("Verified client input size:",os.path.getsize(path))
PY
REMAPPER="$WORK/tiny-remapper-0.14.1-fat.jar"
if [ ! -s "$REMAPPER" ]; then
  curl --fail --location --silent --show-error "https://maven.fabricmc.net/net/fabricmc/tiny-remapper/0.14.1/tiny-remapper-0.14.1-fat.jar" -o "$REMAPPER"
fi
unzip -p "$INTERMEDIARY" mappings/mappings.tiny > "$WORK/mappings.tiny"
"$JAVA" -jar "$REMAPPER" "$WORK/client-official.jar" "$WORK/client-intermediary.jar" "$WORK/mappings.tiny" official intermediary --ignoreConflicts
GENERATOR_OUT="/tmp/pomme-minecraft-api-generator"
mkdir -p "$GENERATOR_OUT"
"$JAVAC" --release 21 -proc:none -cp "$ASM:$ASM_TREE" -d "$GENERATOR_OUT" \
  "$ROOT/pomme-jvm/tools/MinecraftApiStubGenerator.java" \
  "$ROOT/pomme-jvm/tools/MinecraftApiStubMerger.java"
"$JAVA" -Xmx2g -cp "$GENERATOR_OUT:$ASM:$ASM_TREE" MinecraftApiStubGenerator "$WORK/client-intermediary.jar" "$WORK/generated-stubs.jar"
"$JAVA" -Xmx2g -cp "$GENERATOR_OUT:$ASM:$ASM_TREE" MinecraftApiStubMerger "$WORK/generated-stubs.jar" "$BASE" "$SHIM.tmp"
mv "$SHIM.tmp" "$SHIM"
# Preserve verified generation inputs under /tmp for reproducible diagnostics.
echo "Generated and merged Minecraft $MC_VERSION ABI stubs into $SHIM"
