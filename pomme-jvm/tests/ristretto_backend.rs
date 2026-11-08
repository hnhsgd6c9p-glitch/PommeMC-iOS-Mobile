use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use pomme_jvm::ristretto_backend::{run_fabric_entrypoints, run_java_main};

#[tokio::test(flavor = "multi_thread")]
async fn ristretto_executes_arrays_lambdas_reflection_and_exceptions() {
    let Some(java_home) = std::env::var_os("JAVA_HOME").map(PathBuf::from) else {
        eprintln!("skipping Ristretto integration test: JAVA_HOME is not set");
        return;
    };
    let javac = java_home.join("bin").join("javac");
    if !javac.is_file() || !java_home.join("lib").join("modules").is_file() {
        eprintln!("skipping Ristretto integration test: JAVA_HOME is not a complete JDK");
        return;
    }
    let runtime_home = std::env::var_os("POMME_JAVA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| java_home.clone());

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("pomme-ristretto-{}-{nonce}", std::process::id()));
    let classes = root.join("classes");
    fs::create_dir_all(&classes).expect("create fixture classes directory");
    let source = root.join("RistrettoCompatibilityProbe.java");
    fs::write(
        &source,
        r#"
import java.lang.reflect.Field;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import net.fabricmc.api.ModInitializer;

public final class RistrettoCompatibilityProbe {
    private static final int BASE = initialize();

    private static int initialize() {
        return 7;
    }

    private static final class Box {
        private int value;

        private Box(int value) {
            this.value = value;
        }
    }

    public static void main(String[] args) throws Exception {
        int[] values = {1, 2, 3};
        int total = Arrays.stream(values).map(value -> value * BASE).sum();
        if (total != 42) {
            throw new AssertionError("array/lambda/static-init failure: " + total);
        }

        List<String> names = new ArrayList<>();
        names.add(" Pomme ");
        String label = names.stream()
            .map(String::trim)
            .map(value -> value + " JVM")
            .findFirst()
            .orElseThrow();
        if (!"Pomme JVM".equals(label)) {
            throw new AssertionError("method-reference/string failure: " + label);
        }

        Box box = new Box(42);
        Field field = Box.class.getDeclaredField("value");
        field.setAccessible(true);
        if (field.getInt(box) != 42) {
            throw new AssertionError("reflection/instance-field failure");
        }

        boolean caught = false;
        try {
            throw new IllegalArgumentException("expected");
        } catch (IllegalArgumentException expected) {
            caught = "expected".equals(expected.getMessage());
        }
        if (!caught) {
            throw new AssertionError("exception handling failure");
        }
    }
}

final class RistrettoEntrypointProbe implements ModInitializer {
    @Override
    public void onInitialize() {
        String[] values = {"Fabric", "entrypoint"};
        if (!String.join(" ", values).equals("Fabric entrypoint")) {
            throw new AssertionError("Fabric entrypoint bridge failure");
        }
    }
}

final class RistrettoFactoryEntrypointProbe implements ModInitializer {
    private static boolean created;

    private RistrettoFactoryEntrypointProbe() {
        created = true;
    }

    public static RistrettoFactoryEntrypointProbe create() {
        return new RistrettoFactoryEntrypointProbe();
    }

    @Override
    public void onInitialize() {
        if (!created) {
            throw new AssertionError("Fabric static factory was not used");
        }
    }
}
"#,
    )
    .expect("write Java fixture");

    let release_file = fs::read_to_string(java_home.join("release")).unwrap_or_default();
    let target_release = if release_file.contains("JAVA_VERSION=\"25") {
        "25"
    } else {
        "21"
    };
    let fabric_initializer_source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources/net/fabricmc/api/ModInitializer.java");
    let compilation = Command::new(&javac)
        .args(["--release", target_release, "-d"])
        .arg(&classes)
        .arg(&fabric_initializer_source)
        .arg(&source)
        .output()
        .expect("run javac");
    assert!(
        compilation.status.success(),
        "javac failed: {}",
        String::from_utf8_lossy(&compilation.stderr)
    );

    let result = run_java_main(
        &runtime_home,
        std::slice::from_ref(&classes),
        "RistrettoCompatibilityProbe",
        &[],
    )
    .await;

    let entrypoint_result = run_fabric_entrypoints(
        &runtime_home,
        std::slice::from_ref(&classes),
        &[
            (
                "RistrettoEntrypointProbe".to_owned(),
                "onInitialize".to_owned(),
            ),
            (
                "RistrettoFactoryEntrypointProbe::create".to_owned(),
                "onInitialize".to_owned(),
            ),
        ],
    )
    .await;

    let _ = fs::remove_dir_all(&root);
    result.expect("Ristretto must execute the Java compatibility probe");
    entrypoint_result.expect("Ristretto must invoke the Fabric-style initializer");
}
