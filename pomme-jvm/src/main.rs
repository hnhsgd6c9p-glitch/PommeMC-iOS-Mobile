use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args_os();
    let program = args.next().unwrap_or_default();
    let Some(path) = args.next() else {
        eprintln!("Usage: {} <fabric-mod.jar>", std::path::Path::new(&program).display());
        return ExitCode::from(2);
    };
    if args.next().is_some() {
        eprintln!("Expected exactly one JAR path.");
        return ExitCode::from(2);
    }

    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("Could not read {}: {error}", std::path::Path::new(&path).display());
            return ExitCode::FAILURE;
        }
    };

    match pomme_jvm::run_fabric_mod_jar(&bytes, 1_000_000) {
        Ok(report) => {
            println!("Loaded Java Fabric mod: {}", report.mod_id);
            for class in report.initialized_classes {
                println!("  initialized {class}");
            }
            for message in report.logs { println!("[Java mod] {message}"); }\n            println!("Executed {} JVM instruction(s).", report.instructions_executed);
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("Mod could not be executed by the current JVM subset: {error}");
            ExitCode::FAILURE
        }
    }
}
