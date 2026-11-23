#!/bin/sh
set -eu

# Pinned, real third-party Fabric mod used as an integration compatibility
# fixture. The upstream artifact is fetched from Modrinth and SHA-1 verified.
DEST="${1:-/tmp/pomme-third-party-mods}"
mkdir -p "$DEST"
FILE="$DEST/appleskin-fabric-mc1.21-3.0.6.jar"
URL="https://cdn.modrinth.com/data/EsAfCjCV/versions/b5ZiCjAr/appleskin-fabric-mc1.21-3.0.6.jar"
EXPECTED_SHA1="97452cfadfde1f8f8c67838643019eabafa58fbb"

if [ ! -s "$FILE" ] || [ "$(shasum -a 1 "$FILE" | awk '{print $1}')" != "$EXPECTED_SHA1" ]; then
    curl --fail --location --silent --show-error "$URL" -o "$FILE.tmp"
    printf '%s  %s\n' "$EXPECTED_SHA1" "$FILE.tmp" | shasum -a 1 -c -
    mv "$FILE.tmp" "$FILE"
fi
printf 'Verified third-party fixture: %s\n' "$FILE"

# Lithium is a widely used, real third-party Fabric optimization mod. It has
# no Fabric API dependency, making it a useful loader + Mixin compatibility
# target independent of AppleSkin's Cloth Config/Gson initialization.
LITHIUM_FILE="$DEST/lithium-fabric-0.15.4+mc1.21.1.jar"
LITHIUM_URL="https://cdn.modrinth.com/data/gvQqBUqZ/versions/N08Z8wog/lithium-fabric-0.15.4%2Bmc1.21.1.jar"
LITHIUM_SHA1="7d82a004403b7780d4443d8e40361a55cfd0ae63"

if [ ! -s "$LITHIUM_FILE" ] || [ "$(shasum -a 1 "$LITHIUM_FILE" | awk '{print $1}')" != "$LITHIUM_SHA1" ]; then
    curl --fail --location --silent --show-error "$LITHIUM_URL" -o "$LITHIUM_FILE.tmp"
    printf '%s  %s\n' "$LITHIUM_SHA1" "$LITHIUM_FILE.tmp" | shasum -a 1 -c -
    mv "$LITHIUM_FILE.tmp" "$LITHIUM_FILE"
fi
printf 'Verified third-party fixture: %s\n' "$LITHIUM_FILE"
