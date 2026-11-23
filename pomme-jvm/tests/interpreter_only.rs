//! Guards the interpreter-only runtime contract used by Pomme on iOS.
//!
//! `pomme-jvm` depends on `ristretto_vm` with `default-features = false`, so
//! the vendored VM is built without its `jit` feature and the vendored
//! `ristretto_types` does not pull in `ristretto_jit` / Cranelift. The public
//! configuration API must keep working in that build; this test is the
//! lightweight regression check for it (no JDK or Fabric runtime required).

use ristretto_vm::ConfigurationBuilder;

#[test]
fn interpreted_configuration_builds_without_jit_feature() {
    let configuration = ConfigurationBuilder::new()
        .interpreted(true)
        .batch_compilation(false)
        .build()
        .expect("interpreter-only configuration must build without the jit feature");

    assert!(
        configuration.interpreted(),
        "interpreted mode must be preserved in an interpreter-only build"
    );
}
