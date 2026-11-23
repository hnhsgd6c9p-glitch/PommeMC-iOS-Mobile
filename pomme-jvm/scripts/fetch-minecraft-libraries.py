#!/usr/bin/env python3
"""Fetch selected Java-side Minecraft 1.21.x dependencies (never native classifiers)."""
import hashlib
import json
import os
import sys
import urllib.request

if len(sys.argv) != 3:
    raise SystemExit("Usage: fetch-minecraft-libraries.py <minecraft-version> <destination-lib-dir>")
version, destination = sys.argv[1:]
os.makedirs(destination, exist_ok=True)
wanted = {
    "com.google.code.gson:gson", "com.google.guava:failureaccess", "com.google.guava:guava",
    "ca.weblite:java-objc-bridge", "com.github.oshi:oshi-core",
    "com.ibm.icu:icu4j", "com.mojang:authlib", "com.mojang:brigadier",
    "com.mojang:datafixerupper", "com.mojang:blocklist", "com.mojang:logging",
    "com.mojang:text2speech",
    "commons-codec:commons-codec", "commons-io:commons-io", "commons-logging:commons-logging",
    "io.netty:netty-buffer", "io.netty:netty-codec", "io.netty:netty-common",
    "io.netty:netty-handler", "io.netty:netty-resolver", "io.netty:netty-transport",
    "io.netty:netty-transport-classes-epoll", "it.unimi.dsi:fastutil",
    "net.java.dev.jna:jna", "net.java.dev.jna:jna-platform", "net.sf.jopt-simple:jopt-simple",
    "org.apache.commons:commons-compress", "org.apache.commons:commons-lang3",
    "org.apache.httpcomponents:httpclient", "org.apache.httpcomponents:httpcore",
    "org.jcraft:jorbis", "org.joml:joml", "org.lz4:lz4-java",
    "org.lwjgl:lwjgl", "org.lwjgl:lwjgl-freetype", "org.lwjgl:lwjgl-glfw",
    "org.lwjgl:lwjgl-jemalloc", "org.lwjgl:lwjgl-openal", "org.lwjgl:lwjgl-opengl",
    "org.lwjgl:lwjgl-stb", "org.lwjgl:lwjgl-tinyfd",
    "org.apache.logging.log4j:log4j-api", "org.apache.logging.log4j:log4j-core",
    "org.apache.logging.log4j:log4j-slf4j2-impl",
}
manifest = json.load(urllib.request.urlopen("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json"))
entry = next(item for item in manifest["versions"] if item["id"] == version)
metadata = json.load(urllib.request.urlopen(entry["url"]))
selected = []
for library in metadata.get("libraries", []):
    parts = library.get("name", "").split(":")
    if len(parts) != 3:  # Skip native/classifier coordinates entirely.
        continue
    coordinate = ":".join(parts[:2])
    artifact = library.get("downloads", {}).get("artifact")
    if coordinate not in wanted or not artifact:
        continue
    filename = os.path.basename(artifact["path"])
    output = os.path.join(destination, filename)
    expected = artifact.get("sha1")
    valid = False
    if os.path.isfile(output):
        digest = hashlib.sha1(open(output, "rb").read()).hexdigest()
        valid = not expected or digest == expected
    if not valid:
        temp = output + ".tmp"
        print("Fetching", coordinate, filename)
        urllib.request.urlretrieve(artifact["url"], temp)
        digest = hashlib.sha1(open(temp, "rb").read()).hexdigest()
        if expected and digest != expected:
            os.remove(temp)
            raise SystemExit("SHA-1 mismatch for " + filename)
        os.replace(temp, output)
    selected.append({
        "coordinate": library["name"],
        "file": filename,
        "sha1": expected,
        "size": artifact.get("size", os.path.getsize(output)),
    })
missing = wanted - {":".join(item["coordinate"].split(":")[:2]) for item in selected}
if missing:
    print("Optional/unavailable library coordinates:", ", ".join(sorted(missing)))
selected.sort(key=lambda item: item["file"])
manifest_path = os.path.join(os.path.dirname(destination), "minecraft-libraries.json")
with open(manifest_path, "w") as handle:
    json.dump(selected, handle, indent=2)
    handle.write("\n")
print("Staged", len(selected), "Minecraft Java dependency JARs (native classifiers excluded;", round(sum(x["size"] for x in selected)/1048576, 1), "MiB )")
print("Dependency manifest:", manifest_path)
