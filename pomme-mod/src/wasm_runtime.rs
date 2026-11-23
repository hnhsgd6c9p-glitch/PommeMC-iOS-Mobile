//! Minimal, fuel-metered WebAssembly interpreter for native Pomme mods.
//!
//! No WASI filesystem, networking, clocks, or game imports are linked yet.
//! A module can only run a no-argument/no-result export. This intentionally
//! small ABI is a safe starting point while host capabilities are designed.

use thiserror::Error;
use wasmi::{Config, Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder};

const MAX_LINEAR_MEMORY_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum WasmRuntimeError {
    #[error("WebAssembly module error: {0}")]
    Wasmi(#[from] wasmi::Error),
}

struct HostState {
    limits: StoreLimits,
}

/// Reusable interpreter engine. It compiles WebAssembly to Wasmi's portable
/// interpreter representation, not native machine code.
pub struct WasmModRuntime {
    engine: Engine,
}

impl Default for WasmModRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl WasmModRuntime {
    pub fn new() -> Self {
        let mut config = Config::default();
        config.consume_fuel(true);
        Self {
            engine: Engine::new(&config),
        }
    }

    /// Validates a WebAssembly binary without executing it.
    pub fn validate(&self, wasm: &[u8]) -> Result<(), WasmRuntimeError> {
        Module::new(&self.engine, wasm)?;
        Ok(())
    }

    /// Calls a no-argument/no-result export with a deterministic instruction
    /// budget. Imported functions are rejected until a versioned host ABI is
    /// available. Returns the amount of fuel consumed by the call.
    pub fn call_unit_export(
        &self,
        wasm: &[u8],
        export_name: &str,
        fuel: u64,
    ) -> Result<u64, WasmRuntimeError> {
        let module = Module::new(&self.engine, wasm)?;
        let state = HostState {
            limits: StoreLimitsBuilder::new()
                .memory_size(MAX_LINEAR_MEMORY_BYTES)
                .build(),
        };
        let mut store = Store::new(&self.engine, state);
        store.limiter(|state| &mut state.limits);
        store.set_fuel(fuel)?;

        let linker = Linker::<HostState>::new(&self.engine);
        let instance = linker.instantiate_and_start(&mut store, &module)?;
        let function = instance.get_typed_func::<(), ()>(&store, export_name)?;
        function.call(&mut store, ())?;

        Ok(fuel.saturating_sub(store.get_fuel()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Minimal Wasm binary exporting `pomme_init: () -> ()`.
    const EMPTY_INIT_MODULE: &[u8] = &[
        0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, // header
        0x01, 0x04, 0x01, 0x60, 0x00, 0x00, // type [] -> []
        0x03, 0x02, 0x01, 0x00, // one function of type 0
        0x07, 0x0e, 0x01, 0x0a, b'p', b'o', b'm', b'm', b'e', b'_', b'i', b'n', b'i', b't', 0x00,
        0x00, // export function 0
        0x0a, 0x04, 0x01, 0x02, 0x00, 0x0b, // function body
    ];

    #[test]
    fn validates_and_executes_export_with_fuel() {
        let runtime = WasmModRuntime::new();
        runtime.validate(EMPTY_INIT_MODULE).unwrap();
        let consumed = runtime
            .call_unit_export(EMPTY_INIT_MODULE, "pomme_init", 100)
            .unwrap();
        assert!(consumed <= 100);
    }

    #[test]
    fn rejects_invalid_wasm() {
        let runtime = WasmModRuntime::new();
        assert!(runtime.validate(b"not wasm").is_err());
    }

    #[test]
    fn enforces_fuel_budget() {
        let runtime = WasmModRuntime::new();
        assert!(
            runtime
                .call_unit_export(EMPTY_INIT_MODULE, "pomme_init", 0)
                .is_err()
        );
    }
}
