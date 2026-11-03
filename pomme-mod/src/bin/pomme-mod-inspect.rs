use std::process::ExitCode;
use std::{env, fs};

use pomme_mod::{PackageKind, analyze_fabric_jar, compatibility_verdict, inspect_package};

fn main() -> ExitCode {
    let Some(path) = env::args_os().nth(1) else {
        eprintln!("Usage: pomme-mod-inspect <fabric-mod.jar>");
        return ExitCode::from(2);
    };
    if path == "--help" || path == "-h" {
        println!(
            "Usage: pomme-mod-inspect <fabric-mod.jar>\n\nScans Fabric metadata and Java class files without loading or executing the mod."
        );
        return ExitCode::SUCCESS;
    }
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("Could not read {}: {error}", path.to_string_lossy());
            return ExitCode::FAILURE;
        }
    };
    let package = match inspect_package(&bytes) {
        Ok(package) => package,
        Err(error) => {
            eprintln!("Package inspection failed: {error}");
            return ExitCode::FAILURE;
        }
    };

    println!("Archive entries: {}", package.archive_entries);
    println!("Java classes: {}", package.class_files);
    println!("WASM modules: {}", package.wasm_files);
    println!("Mixin configs: {}", package.has_mixin_configs);
    println!("Nested JAR declarations: {}", package.nested_jar_count);

    match &package.kind {
        PackageKind::FabricJava(metadata) => {
            println!("Mod: {} {}", metadata.id, metadata.version);
            println!("Name: {}", metadata.name.as_deref().unwrap_or("(unnamed)"));
            println!("Dependencies: {}", metadata.depends.len());
            println!("Entrypoint groups: {}", metadata.entrypoints.len());
            println!("Initial classification: {:?}", package.compatibility());
            match analyze_fabric_jar(&bytes) {
                Ok(report) => {
                    println!(
                        "Porting verdict: {:?}",
                        compatibility_verdict(&package, &report)
                    );
                    println!("\nJava bytecode inventory");
                    println!("Analyzed classes: {}", report.analyzed_classes);
                    println!(
                        "Skipped/malformed/oversized classes: {}",
                        report.skipped_classes
                    );
                    println!("Class-file versions: {:?}", report.class_versions);
                    println!("Detected features: {:?}", report.feature_flags);
                    println!("invokedynamic sites: {}", report.invokedynamic_sites);
                    println!(
                        "invokedynamic constants: {}",
                        report.invokedynamic_constants
                    );
                    println!("Dynamic constants: {}", report.dynamic_constants);
                    println!(
                        "Method-handle constants: {}",
                        report.method_handle_constants
                    );
                    println!("Method-type constants: {}", report.method_type_constants);
                    println!("Bootstrap methods: {}", report.bootstrap_methods);
                    println!(
                        "Distinct referenced classes: {}",
                        report.referenced_classes.len()
                    );
                    println!("Distinct invoked methods: {}", report.invoked_methods.len());
                    println!(
                        "High-risk/unsupported API references: {}",
                        report.unsupported_or_high_risk_apis.len()
                    );
                    for method in report.unsupported_or_high_risk_apis.iter().take(20) {
                        println!("  - {method}");
                    }
                    println!("\nOpcode frequency (top 20)");
                    let mut opcodes: Vec<_> = report.opcode_counts.iter().collect();
                    opcodes.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
                    for (opcode, count) in opcodes.into_iter().take(20) {
                        println!("  {opcode}: {count}");
                    }
                }
                Err(error) => {
                    eprintln!("Bytecode analysis failed: {error}");
                    return ExitCode::FAILURE;
                }
            }
        }
        PackageKind::NativeWasm(metadata) => {
            println!("Native Pomme mod: {} {}", metadata.id, metadata.version);
            println!("Module: {}", metadata.module);
            println!("Compatibility: NativeWasm");
        }
    }
    println!("\nNote: inspection does not mean the mod can currently be loaded into the game.");
    ExitCode::SUCCESS
}
