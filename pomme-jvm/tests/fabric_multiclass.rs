use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use pomme_jvm::{
    ModExecutionError, run_fabric_mod_jar, run_fabric_mod_jar_with_classpath,
    run_fabric_mod_jar_with_classpath_and_version,
};
use pomme_mod::analyze_class;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

fn compile_fabric_fixture() -> Option<(PathBuf, PathBuf, Vec<u8>)> {
    let javac = match Command::new("javac").arg("--version").output() {
        Ok(output) if output.status.success() => true,
        Ok(_) => false,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => panic!("could not launch javac: {error}"),
    };
    if !javac {
        eprintln!("Skipping multi-class Fabric fixture: javac is not installed");
        return None;
    }

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before UNIX epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "pomme-jvm-multiclass-{}-{unique}",
        std::process::id()
    ));
    let classes = root.join("classes");
    std::fs::create_dir_all(&classes).expect("create fixture output directory");

    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/multiclass");
    let sources = [
        fixture.join("net/fabricmc/api/ModInitializer.java"),
        fixture.join("net/fabricmc/api/ClientModInitializer.java"),
        fixture.join("net/fabricmc/api/EnvType.java"),
        fixture.join("net/fabricmc/loader/api/entrypoint/PreLaunchEntrypoint.java"),
        fixture.join("pomme/api/Pomme.java"),
        fixture.join("org/slf4j/Logger.java"),
        fixture.join("org/slf4j/LoggerFactory.java"),
        fixture.join("net/fabricmc/loader/api/FabricLoader.java"),
        fixture.join("net/minecraft/client/Minecraft.java"),
        fixture.join("example/ExampleMod.java"),
        fixture.join("example/MinecraftVersionProbe.java"),
        fixture.join("example/ClientMod.java"),
        fixture.join("example/PreLaunchMod.java"),
        fixture.join("example/Helper.java"),
        fixture.join("example/Counter.java"),
        fixture.join("example/BaseCounter.java"),
        fixture.join("example/ConstantFields.java"),
        fixture.join("example/ArrayFixture.java"),
        fixture.join("example/MarkerException.java"),
        fixture.join("example/Value.java"),
    ];
    let result = Command::new("javac")
        .arg("--release")
        .arg("21")
        .arg("-d")
        .arg(&classes)
        .args(&sources)
        .output()
        .expect("launch javac for multi-class fixture");
    assert!(
        result.status.success(),
        "javac failed:\n{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );

    let mut writer = ZipWriter::new(std::io::Cursor::new(Vec::new()));
    writer
        .start_file("fabric.mod.json", SimpleFileOptions::default())
        .expect("start Fabric metadata");
    writer
        .write_all(
            br#"{"schemaVersion":1,"id":"multiclass_fixture","version":"1.0.0","entrypoints":{"preLaunch":["example.PreLaunchMod"],"main":[{"value":"example.ExampleMod","adapter":"default"}],"client":[{"value":"example.ServerOnly","environment":"server"},{"value":"example.ClientMod","environment":"client"},{"value":"example.MinecraftVersionProbe","environment":"client"}],"server":["example.ServerOnly"]},"jars":[{"file":"META-INF/jars/helper.jar"}]}"#,
        )
        .expect("write Fabric metadata");

    for relative in [
        "example/ExampleMod.class",
        "example/ClientMod.class",
        "example/PreLaunchMod.class",
        "example/MinecraftVersionProbe.class",
        "net/fabricmc/api/ModInitializer.class",
        "net/fabricmc/api/ClientModInitializer.class",
        "net/fabricmc/loader/api/entrypoint/PreLaunchEntrypoint.class",
        "pomme/api/Pomme.class",
    ] {
        writer
            .start_file(relative, SimpleFileOptions::default())
            .expect("start class entry");
        let bytes = std::fs::read(classes.join(relative)).expect("read compiled class");
        writer.write_all(&bytes).expect("write class entry");
    }
    let mut nested = ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for relative in [
        "example/Helper.class",
        "example/Counter.class",
        "example/BaseCounter.class",
        "example/Value.class",
        "example/ArrayFixture.class",
        "example/MarkerException.class",
    ] {
        nested
            .start_file(relative, SimpleFileOptions::default())
            .expect("start nested helper class");
        let helper =
            std::fs::read(classes.join(relative)).expect("read compiled nested helper class");
        nested
            .write_all(&helper)
            .expect("write nested helper class");
    }
    let nested_bytes = nested
        .finish()
        .expect("finish nested helper JAR")
        .into_inner();

    writer
        .start_file("META-INF/jars/helper.jar", SimpleFileOptions::default())
        .expect("start nested JAR entry");
    writer
        .write_all(&nested_bytes)
        .expect("write nested JAR entry");

    let jar = writer.finish().expect("finish fixture JAR").into_inner();
    Some((root, classes, jar))
}

#[test]
fn loads_and_executes_static_helper_from_a_fabric_nested_jar() {
    let Some((root, classes, jar)) = compile_fabric_fixture() else {
        return;
    };

    let helper_analysis = analyze_class(
        &std::fs::read(classes.join("example/Helper.class")).expect("read helper class"),
    )
    .expect("analyze helper fields");
    assert!(helper_analysis.declared_fields.iter().any(|field| {
        field.name == "sharedCount" && field.descriptor == "I" && field.access_flags & 0x0008 != 0
    }));
    assert!(helper_analysis.declared_fields.iter().any(|field| {
        field.name == "instanceCount" && field.descriptor == "I" && field.access_flags & 0x0008 == 0
    }));
    assert!(helper_analysis.invokedynamic_sites > 0);
    let constants = analyze_class(
        &std::fs::read(classes.join("example/ConstantFields.class")).expect("read constant class"),
    )
    .expect("analyze constant field");
    assert!(
        constants
            .declared_fields
            .iter()
            .any(|field| { field.name == "MAGIC" && field.constant_value == Some(42) })
    );
    let array_fixture = analyze_class(
        &std::fs::read(classes.join("example/ArrayFixture.class")).expect("read array fixture"),
    )
    .expect("analyze exception fixture");
    assert_eq!(
        array_fixture
            .methods
            .iter()
            .find(|method| method.name == "caughtException")
            .expect("caught exception method")
            .exception_handlers
            .len(),
        1
    );

    let report = run_fabric_mod_jar(&jar, 10_000)
        .expect("execute Fabric entrypoint that calls a second class");
    assert_eq!(report.mod_id, "multiclass_fixture");
    assert_eq!(
        report.initialized_classes,
        [
            "example.PreLaunchMod",
            "example.ExampleMod",
            "example.ClientMod",
            "example.MinecraftVersionProbe"
        ]
    );
    assert_eq!(
        report.logs,
        [
            "pre-launch entrypoint executed",
            "example/ExampleMod: SLF4J bridge works",
            "example/ExampleMod: client environment bridge works",
            "7",
            "41",
            "static string field works",
            "10",
            "4",
            "14",
            "33",
            "-1",
            "22",
            "1",
            "0",
            "42",
            "99",
            "4",
            "array reference works",
            "1",
            "-1",
            "first",
            "second",
            "10",
            "10",
            "12",
            "values=10/12",
            "5",
            "first",
            "second",
            "12",
            "12",
            "14",
            "values=12/14",
            "Cross-class Java execution works",
            "client entrypoint executed",
            "unknown"
        ],
        "the helper's Pomme bridge call should run through the shared class path"
    );
    assert!(report.instructions_executed >= 10);

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn minecraft_api_bridge_exposes_the_active_game_version() {
    let Some((root, _classes, jar)) = compile_fabric_fixture() else {
        return;
    };
    let report = run_fabric_mod_jar_with_classpath_and_version(&jar, &[], 10_000, "1.21.1")
        .expect("execute a client entrypoint using Minecraft.getInstance");
    assert_eq!(report.logs.last().map(String::as_str), Some("1.21.1"));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn static_integer_helpers_share_the_entrypoint_fuel_budget() {
    let Some((root, _classes, jar)) = compile_fabric_fixture() else {
        return;
    };
    let full_run = run_fabric_mod_jar(&jar, 10_000).unwrap();
    let helper_fuel_exhausted = (1..full_run.instructions_executed).any(|fuel| {
        matches!(
            run_fabric_mod_jar(&jar, fuel),
            Err(ModExecutionError::Vm(pomme_jvm::VmError::FuelExhausted))
        )
    });
    assert!(
        helper_fuel_exhausted,
        "the nested integer helper must consume the same bounded entrypoint budget"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn resolves_classes_from_an_initialized_separate_dependency_mod() {
    let Some((root, classes, _nested_fixture)) = compile_fabric_fixture() else {
        return;
    };

    let mut library = ZipWriter::new(std::io::Cursor::new(Vec::new()));
    library
        .start_file("fabric.mod.json", SimpleFileOptions::default())
        .unwrap();
    library
        .write_all(br#"{"schemaVersion":1,"id":"helper_lib","version":"1.2.0"}"#)
        .unwrap();
    for relative in [
        "example/Helper.class",
        "example/Counter.class",
        "example/BaseCounter.class",
        "example/Value.class",
        "example/ArrayFixture.class",
        "example/MarkerException.class",
    ] {
        library
            .start_file(relative, SimpleFileOptions::default())
            .unwrap();
        library
            .write_all(&std::fs::read(classes.join(relative)).unwrap())
            .unwrap();
    }
    let library = library.finish().unwrap().into_inner();

    let mut main = ZipWriter::new(std::io::Cursor::new(Vec::new()));
    main.start_file("fabric.mod.json", SimpleFileOptions::default())
        .unwrap();
    main.write_all(
        br#"{"schemaVersion":1,"id":"dependent_mod","version":"1.0.0","depends":{"helper_lib":"^1.2.0"},"entrypoints":{"main":["example.ExampleMod"]}}"#,
    )
    .unwrap();
    for relative in [
        "example/ExampleMod.class",
        "net/fabricmc/api/ModInitializer.class",
        "pomme/api/Pomme.class",
    ] {
        main.start_file(relative, SimpleFileOptions::default())
            .unwrap();
        main.write_all(&std::fs::read(classes.join(relative)).unwrap())
            .unwrap();
    }
    let main = main.finish().unwrap().into_inner();

    let report = run_fabric_mod_jar_with_classpath(&main, &[&library], 10_000)
        .expect("dependent mod should resolve helper class from library mod");
    assert_eq!(report.mod_id, "dependent_mod");
    assert_eq!(
        report.logs,
        [
            "example/ExampleMod: SLF4J bridge works",
            "example/ExampleMod: client environment bridge works",
            "example/ExampleMod: FabricLoader dependency lookup works",
            "7",
            "41",
            "static string field works",
            "10",
            "4",
            "14",
            "33",
            "-1",
            "22",
            "1",
            "0",
            "42",
            "99",
            "4",
            "array reference works",
            "1",
            "-1",
            "first",
            "second",
            "10",
            "10",
            "12",
            "values=10/12",
            "5",
            "first",
            "second",
            "12",
            "12",
            "14",
            "values=12/14",
            "Cross-class Java execution works"
        ]
    );

    let library_class_count = pomme_jvm::validate_fabric_mod_jar(&library).unwrap();
    assert_eq!(library_class_count, 6);
    let _ = std::fs::remove_dir_all(root);
}
