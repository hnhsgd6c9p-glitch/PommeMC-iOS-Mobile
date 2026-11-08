use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use pomme_jvm::ristretto_backend::run_fabric_loader;

fn run(command: &mut Command, what: &str) {
    let output = command
        .output()
        .unwrap_or_else(|error| panic!("{what}: {error}"));
    assert!(
        output.status.success(),
        "{what} failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn path_arg(path: &Path) -> &std::ffi::OsStr {
    path.as_os_str()
}

#[tokio::test(flavor = "multi_thread")]
async fn real_fabric_mod_receives_client_tick_events_in_persistent_ristretto_vm() {
    let Some(java_home) = std::env::var_os("JAVA_HOME").map(PathBuf::from) else {
        eprintln!("skipping Fabric mod integration test: JAVA_HOME is not set");
        return;
    };
    let Some(fabric_runtime) = std::env::var_os("POMME_FABRIC_RUNTIME").map(PathBuf::from) else {
        eprintln!("skipping Fabric mod integration test: POMME_FABRIC_RUNTIME is not set");
        return;
    };

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    let game_dir = std::env::temp_dir().join(format!(
        "pomme-fabric-tick-probe-{}-{nonce}",
        std::process::id()
    ));
    let mods_dir = game_dir.join("mods");
    let source_dir = game_dir.join("fixture-src");
    let classes_dir = game_dir.join("fixture-classes");
    std::fs::create_dir_all(&mods_dir).expect("create mods directory");
    std::fs::create_dir_all(&source_dir).expect("create fixture source directory");
    std::fs::create_dir_all(&classes_dir).expect("create fixture classes directory");

    let lib_dir = fabric_runtime.join("lib");
    let loader_jar = lib_dir.join("fabric-loader-0.18.4-pomme.jar");
    let shim_jar = fabric_runtime.join("builtin-mods/pomme-fabric-api-shim.jar");
    let mixinextras_jar = fabric_runtime.join("builtin-mods/mixinextras-fabric-0.5.0.jar");
    std::fs::copy(&shim_jar, mods_dir.join("pomme-fabric-api-shim.jar"))
        .expect("install Pomme Fabric API bridge");
    std::fs::copy(
        &mixinextras_jar,
        mods_dir.join("mixinextras-fabric-0.5.0.jar"),
    )
    .expect("install MixinExtras as a standalone mod");

    let java_source = source_dir.join("ProbeMod.java");
    std::fs::write(
        &java_source,
        r#"package probe;
import net.fabricmc.api.ClientModInitializer;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientTickEvents;
import net.minecraft.class_310;
public final class ProbeMod implements ClientModInitializer {
    private static int ticks;
    @Override public void onInitializeClient() {
        ClientTickEvents.START_CLIENT_TICK.register((class_310 client) -> {
            ticks++;
            System.setProperty("pomme.probe.ticks", Integer.toString(ticks));
            System.setProperty("pomme.probe.x", Double.toString(client.field_1724.method_23317()));
            System.setProperty("pomme.probe.health", Float.toString(client.field_1724.method_6032()));
        });
        ClientTickEvents.START_WORLD_TICK.register(world -> {
            int count = Integer.parseInt(System.getProperty("pomme.probe.worldTicks", "0")) + 1;
            System.setProperty("pomme.probe.worldTicks", Integer.toString(count));
        });
        ClientTickEvents.END_WORLD_TICK.register(world -> {
            int count = Integer.parseInt(System.getProperty("pomme.probe.endWorldTicks", "0")) + 1;
            System.setProperty("pomme.probe.endWorldTicks", Integer.toString(count));
        });
        ClientTickEvents.END_CLIENT_TICK.register(client -> {
            int count = Integer.parseInt(System.getProperty("pomme.probe.endClientTicks", "0")) + 1;
            System.setProperty("pomme.probe.endClientTicks", Integer.toString(count));
        });
    }
    public static int getTicks() { return ticks; }
}
"#,
    )
    .expect("write Fabric probe source");
    let mixin_source = source_dir.join("MinecraftClientMixin.java");
    std::fs::write(
        &mixin_source,
        include_str!("fixtures/fabric-mixin-probe/MinecraftClientMixin.java"),
    )
    .expect("write Mixin probe source");

    let mixinextras_jar = fabric_runtime.join("builtin-mods/mixinextras-fabric-0.5.0.jar");
    let official_api_classpath =
        std::fs::read_to_string("/tmp/pomme-fabric-api-test-classpath-1.21.1")
            .unwrap_or_else(|_| shim_jar.display().to_string());
    let classpath = format!(
        "{}:{}:{}:{}:{}/*",
        loader_jar.display(),
        official_api_classpath.trim(),
        shim_jar.display(),
        mixinextras_jar.display(),
        lib_dir.display()
    );
    run(
        Command::new(java_home.join("bin/javac"))
            .arg("-proc:none")
            .arg("--release")
            .arg("21")
            .arg("-cp")
            .arg(classpath)
            .arg("-d")
            .arg(path_arg(&classes_dir))
            .arg(path_arg(&java_source))
            .arg(path_arg(&mixin_source)),
        "compile real Fabric mod and Mixin fixture",
    );
    std::fs::write(
        classes_dir.join("pomme_tick_probe.mixins.json"),
        include_str!("fixtures/fabric-mixin-probe/pomme_tick_probe.mixins.json"),
    )
    .expect("write Mixin configuration");
    std::fs::write(
        classes_dir.join("fabric.mod.json"),
        r#"{
          "schemaVersion": 1,
          "id": "pomme_tick_probe",
          "version": "1.0.0",
          "environment": "client",
          "entrypoints": { "client": ["probe.ProbeMod"] },
          "mixins": ["pomme_tick_probe.mixins.json"],
          "depends": {
            "fabricloader": ">=0.18.4",
            "fabric-api": ">=0.116.9",
            "minecraft": "1.21.1",
            "java": ">=21"
          }
        }"#,
    )
    .expect("write Fabric mod metadata");

    let probe_jar = mods_dir.join("pomme-tick-probe.jar");
    run(
        Command::new(java_home.join("bin/jar"))
            .arg("--create")
            .arg("--file")
            .arg(path_arg(&probe_jar))
            .arg("-C")
            .arg(path_arg(&classes_dir))
            .arg("."),
        "package real Fabric mod fixture",
    );

    let result =
        run_fabric_loader(&java_home, &fabric_runtime, &game_dir, &mods_dir, "1.21.1").await;
    let session = result.expect("Fabric Loader and real mod entrypoint must initialize");
    session
        .set_system_property(
            "pomme.state",
            "12.5,65.0,-4.25,17.5,20.0,18,2,0,0,0,0,true,false,false,false,false,false,0,false",
        )
        .await
        .expect("publish native player snapshot to Java compatibility layer");

    for _ in 0..3 {
        session
            .invoke_static_void("PommeGameProvider", "dispatchClientTickStart")
            .await
            .expect("dispatch Fabric START_CLIENT_TICK");
        session
            .invoke_static_void("PommeGameProvider", "dispatchClientTickEnd")
            .await
            .expect("dispatch Fabric END_CLIENT_TICK");
    }
    let ticks = session
        .invoke_static_i32("PommeGameProvider", "getProbeTicks")
        .await
        .expect("read Fabric callback counter");
    let mixin_applied = session
        .invoke_static_i32("PommeGameProvider", "isProbeMixinApplied")
        .await
        .expect("inspect transformed Minecraft client class");
    let player_x = session
        .invoke_static_i32("PommeGameProvider", "getProbeX")
        .await
        .expect("read bridged player X coordinate");
    let player_health = session
        .invoke_static_i32("PommeGameProvider", "getProbeHealth")
        .await
        .expect("read bridged player health");
    let world_ticks = session
        .invoke_static_i32("PommeGameProvider", "getProbeWorldTicks")
        .await
        .expect("read Fabric world start ticks");
    let end_world_ticks = session
        .invoke_static_i32("PommeGameProvider", "getProbeEndWorldTicks")
        .await
        .expect("read Fabric world end ticks");
    let end_client_ticks = session
        .invoke_static_i32("PommeGameProvider", "getProbeEndClientTicks")
        .await
        .expect("read Fabric client end ticks");
    let _ = std::fs::remove_dir_all(&game_dir);
    assert_eq!(
        ticks, 3,
        "Fabric client tick callbacks must run once per Pomme tick"
    );
    assert_eq!(
        mixin_applied, 1,
        "Mixin must transform the Minecraft client shim"
    );
    assert_eq!(
        player_x, 12,
        "Minecraft entity API must read Pomme's player position"
    );
    assert_eq!(
        player_health, 17,
        "Minecraft living entity API must read Pomme's player health"
    );
    assert_eq!(world_ticks, 3, "Fabric START_WORLD_TICK must dispatch");
    assert_eq!(end_world_ticks, 3, "Fabric END_WORLD_TICK must dispatch");
    assert_eq!(end_client_ticks, 3, "Fabric END_CLIENT_TICK must dispatch");
}
