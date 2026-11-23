use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use pomme_mod::{
    CompatibilityVerdict, FabricMetadata, PackageKind, PackageReport, analyze_class,
    analyze_jar_classes, compatibility_verdict, execute_static_i32,
};

fn compile_fixtures() -> Option<PathBuf> {
    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let output = match Command::new("javac").arg("--version").output() {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("Skipping javac fixture checks: javac is not installed");
            return None;
        }
        Err(error) => panic!("could not launch javac: {error}"),
    };
    if !output.status.success()
        && String::from_utf8_lossy(&output.stderr).contains("Unable to locate a Java Runtime")
    {
        eprintln!("Skipping javac fixture checks: no JDK is installed");
        return None;
    }
    assert!(output.status.success(), "javac --version failed");

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before UNIX epoch")
        .as_nanos();
    let output_dir =
        std::env::temp_dir().join(format!("pomme-mod-javac-{}-{unique}", std::process::id()));
    std::fs::create_dir_all(&output_dir).expect("create temporary javac output directory");
    let result = Command::new("javac")
        .arg("--release")
        .arg("8")
        .arg("-d")
        .arg(&output_dir)
        .arg(fixture_dir.join("Arithmetic.java"))
        .arg(fixture_dir.join("DynamicLambda.java"))
        .output()
        .expect("launch javac");
    assert!(
        result.status.success(),
        "javac fixture compilation failed:\n{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    Some(output_dir)
}

#[test]
fn compiles_release_8_helpers_and_executes_integer_subset() {
    let Some(output_dir) = compile_fixtures() else {
        return;
    };
    let class_path = output_dir.join("fixtures/Arithmetic.class");
    let bytes = std::fs::read(&class_path).expect("read compiled Arithmetic.class");
    let analysis = analyze_class(&bytes).expect("analyze javac Arithmetic fixture");
    assert_eq!(analysis.major_version, 52);
    assert!(analysis.methods.iter().any(|method| method.name == "sumTo"));
    assert_eq!(
        execute_static_i32(&bytes, "add", "(II)I", &[8, 13], 100),
        Ok(21)
    );
    assert_eq!(
        execute_static_i32(&bytes, "sumTo", "(I)I", &[5], 100),
        Ok(15)
    );

    let report = analyze_jar_classes([("fixtures/Arithmetic.class", bytes)]);
    assert_eq!(report.analyzed_classes, 1);
    assert_eq!(report.skipped_classes, 0);
    assert!(!report.feature_flags.contains("invokedynamic"));

    let _ = std::fs::remove_dir_all(output_dir);
}

#[test]
fn identifies_javac_lambda_bootstrap_structures_as_hard_java() {
    let Some(output_dir) = compile_fixtures() else {
        return;
    };
    let class_path = output_dir.join("fixtures/DynamicLambda.class");
    let bytes = std::fs::read(&class_path).expect("read compiled DynamicLambda.class");
    let analysis = analyze_class(&bytes).expect("analyze javac lambda fixture");
    assert!(analysis.invokedynamic_sites > 0);
    assert!(analysis.invokedynamic_constants > 0);
    assert!(analysis.method_handle_constants > 0);
    assert!(analysis.bootstrap_methods > 0);

    let report = analyze_jar_classes([("fixtures/DynamicLambda.class", bytes)]);
    for feature in [
        "invokedynamic",
        "invokedynamic_constants",
        "method_handles",
        "bootstrap_methods",
    ] {
        assert!(report.feature_flags.contains(feature), "missing {feature}");
    }
    let package = PackageReport {
        kind: PackageKind::FabricJava(Box::new(FabricMetadata {
            schema_version: 1,
            id: "lambda_fixture".into(),
            version: "1".into(),
            name: None,
            description: None,
            environment: None,
            entrypoints: Default::default(),
            depends: Default::default(),
            breaks: Default::default(),
            recommends: Default::default(),
            suggests: Default::default(),
            conflicts: Default::default(),
            provides: Vec::new(),
            jars: Vec::new(),
            mixins: Vec::new(),
            access_widener: None,
        })),
        archive_entries: 2,
        class_files: 1,
        wasm_files: 0,
        has_mixin_configs: false,
        nested_jar_count: 0,
    };
    assert_eq!(
        compatibility_verdict(&package, &report),
        CompatibilityVerdict::HardJava
    );

    let _ = std::fs::remove_dir_all(output_dir);
}
