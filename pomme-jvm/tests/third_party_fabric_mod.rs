use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use pomme_jvm::ristretto_backend::run_fabric_loader;
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

fn copy_mod(source: &Path, destination: &Path) {
    std::fs::copy(source, destination).unwrap_or_else(|error| {
        panic!(
            "copy {} to {}: {error}",
            source.display(),
            destination.display()
        )
    });
}

fn build_minecraft_bridge(shim: &Path, output: &Path) {
    let source = std::fs::File::open(shim).expect("open generated Minecraft ABI shim");
    let mut archive = ZipArchive::new(source).expect("read generated Minecraft ABI shim");
    let destination = std::fs::File::create(output).expect("create standalone Minecraft bridge");
    let mut writer = ZipWriter::new(destination);
    let options = SimpleFileOptions::default();
    let mut copied = 0usize;

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).expect("read shim entry");
        let name = entry.name().expect("read shim entry name").to_owned();
        let is_bridge_class = name.starts_with("net/minecraft/")
            || name.starts_with("com/mojang/")
            || name.starts_with("pomme/bridge/");
        if !is_bridge_class || !name.ends_with(".class") {
            continue;
        }
        writer
            .start_file(&name, options)
            .expect("start bridge class");
        std::io::copy(&mut entry, &mut writer).expect("copy bridge class");
        copied += 1;
    }

    writer
        .start_file("fabric.mod.json", options)
        .expect("start bridge metadata");
    writer
        .write_all(
            br#"{
              "schemaVersion": 1,
              "id": "pomme-minecraft-api-bridge",
              "version": "1.21.1-pomme.1",
              "name": "Pomme Minecraft API Bridge",
              "environment": "*"
            }"#,
        )
        .expect("write bridge metadata");
    writer.finish().expect("finish bridge JAR");
    assert!(
        copied >= 8_000,
        "expected the generated Minecraft ABI, copied {copied} classes"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "AppleSkin currently stalls during Cloth Config/Gson initialization on Ristretto; keep as a manual regression target"]
async fn loads_real_third_party_appleskin_with_official_fabric_api() {
    let Some(java_home) = std::env::var_os("JAVA_HOME").map(PathBuf::from) else {
        eprintln!("skipping third-party Fabric mod test: JAVA_HOME is not set");
        return;
    };
    let Some(fabric_runtime) = std::env::var_os("POMME_FABRIC_RUNTIME").map(PathBuf::from) else {
        eprintln!("skipping third-party Fabric mod test: POMME_FABRIC_RUNTIME is not set");
        return;
    };
    let Some(appleskin_jar) = std::env::var_os("POMME_APPLESKIN_JAR").map(PathBuf::from) else {
        eprintln!("skipping third-party Fabric mod test: POMME_APPLESKIN_JAR is not set");
        return;
    };
    let Some(official_api_jar) =
        std::env::var_os("POMME_OFFICIAL_FABRIC_API_JAR").map(PathBuf::from)
    else {
        eprintln!("skipping third-party Fabric mod test: POMME_OFFICIAL_FABRIC_API_JAR is not set");
        return;
    };

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    let game_dir = std::env::temp_dir().join(format!(
        "pomme-third-party-fabric-{}-{nonce}",
        std::process::id()
    ));
    let mods_dir = game_dir.join("mods");
    std::fs::create_dir_all(&mods_dir).expect("create temporary mods directory");

    let bridge_jar = game_dir.join("pomme-minecraft-api-bridge.jar");
    build_minecraft_bridge(
        &fabric_runtime.join("builtin-mods/pomme-fabric-api-shim.jar"),
        &bridge_jar,
    );
    copy_mod(
        &bridge_jar,
        &mods_dir.join("pomme-minecraft-api-bridge.jar"),
    );
    copy_mod(
        &official_api_jar,
        &mods_dir.join("fabric-api-0.116.9+1.21.1.jar"),
    );
    copy_mod(
        &appleskin_jar,
        &mods_dir.join("appleskin-fabric-mc1.21-3.0.6.jar"),
    );
    copy_mod(
        &fabric_runtime.join("builtin-mods/mixinextras-fabric-0.5.0.jar"),
        &mods_dir.join("mixinextras-fabric-0.5.0.jar"),
    );

    let result =
        run_fabric_loader(&java_home, &fabric_runtime, &game_dir, &mods_dir, "1.21.1").await;
    let session = result.unwrap_or_else(|error| {
        panic!("official Fabric API and third-party AppleSkin failed to initialize: {error}")
    });

    assert_eq!(
        session
            .invoke_static_i32("PommeGameProvider", "isAppleSkinLoaded")
            .await
            .unwrap(),
        1,
        "Fabric Loader must discover AppleSkin as a real third-party mod"
    );
    assert_eq!(
        session
            .invoke_static_i32("PommeGameProvider", "isOfficialFabricApiLoaded")
            .await
            .unwrap(),
        1,
        "Fabric Loader must load the official Fabric API aggregate"
    );
    assert_eq!(
        session
            .invoke_static_i32("PommeGameProvider", "isAppleSkinClassAvailable")
            .await
            .unwrap(),
        1,
        "AppleSkin's real client entrypoint class must be loadable through Knot"
    );

    let _ = std::fs::remove_dir_all(&game_dir);
}
