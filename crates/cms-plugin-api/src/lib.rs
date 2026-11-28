use async_trait::async_trait;
use cms_core::{ApiError, ApiResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use wasmi::{Config, Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder};

const WASM_MAX_BINARY_BYTES: usize = 4 * 1024 * 1024; // 4 MB cap
const WASM_MAX_MEMORY_BYTES: usize = 4 * 1024 * 1024; // 4 MB linear memory cap
const WASM_DEFAULT_FUEL: u64 = 250_000; // Deterministic instruction budget per call

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginDescriptor {
    pub name: String,
    pub version: String,
    pub description: String,
    pub runtime: PluginRuntimeKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginRuntimeKind {
    Native,
    Wasm,
}

#[async_trait]
pub trait CmsPlugin: Send + Sync {
    fn descriptor(&self) -> PluginDescriptor;

    async fn on_boot(&self) -> ApiResult<()> {
        Ok(())
    }

    async fn invoke(&self, action: &str, payload: &serde_json::Value) -> ApiResult<serde_json::Value> {
        Ok(serde_json::json!({
            "plugin": self.descriptor().name,
            "action": action,
            "echo": payload
        }))
    }
}

/// Built-in native SEO & Readability Analyzer plugin.
struct SeoAnalyzerNativePlugin;

#[async_trait]
impl CmsPlugin for SeoAnalyzerNativePlugin {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            name: "rapi::seo-analyzer".into(),
            version: "0.1.0".into(),
            description: "Native Rust SEO title, meta-length, and word-density analyzer.".into(),
            runtime: PluginRuntimeKind::Native,
        }
    }

    async fn invoke(&self, _action: &str, payload: &serde_json::Value) -> ApiResult<serde_json::Value> {
        let text = payload
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let title = payload
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let words = text.split_whitespace().count();
        let reading_minutes = ((words as f64) / 200.0).ceil() as u64;
        let title_ok = (30..=60).contains(&title.chars().count());
        let score = match (words >= 50, title_ok) {
            (true, true) => 95,
            (true, false) => 78,
            (false, true) => 72,
            (false, false) => 50,
        };
        Ok(serde_json::json!({
            "wordCount": words,
            "estimatedReadingMinutes": reading_minutes.max(1),
            "titleOptimalLength": title_ok,
            "seoScore": score,
        }))
    }
}

/// Built-in core capability descriptor plugin.
struct CoreCapabilityPlugin {
    name: &'static str,
    description: &'static str,
}

#[async_trait]
impl CmsPlugin for CoreCapabilityPlugin {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            name: self.name.into(),
            version: "0.1.0".into(),
            description: self.description.into(),
            runtime: PluginRuntimeKind::Native,
        }
    }
}

#[derive(Clone)]
pub struct PluginRegistry {
    plugins: HashMap<String, Arc<dyn CmsPlugin>>,
    wasm_host: WasmPluginHost,
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginRegistry {
    pub fn new() -> Self {
        let mut reg = Self {
            plugins: HashMap::new(),
            wasm_host: WasmPluginHost::new(),
        };
        reg.register(Arc::new(CoreCapabilityPlugin {
            name: "rapi::i18n",
            description: "Built-in Strapi v5 multi-locale content localization engine.",
        }));
        reg.register(Arc::new(CoreCapabilityPlugin {
            name: "rapi::upload",
            description: "Local & cloud media asset storage and image variant pipeline.",
        }));
        reg.register(Arc::new(CoreCapabilityPlugin {
            name: "rapi::graphql",
            description: "Hot-swapped dynamic Strapi v5 GraphQL schema engine.",
        }));
        reg.register(Arc::new(SeoAnalyzerNativePlugin));
        reg
    }

    pub fn register(&mut self, plugin: Arc<dyn CmsPlugin>) {
        let desc = plugin.descriptor();
        self.plugins.insert(desc.name, plugin);
    }

    pub fn descriptors(&self) -> Vec<PluginDescriptor> {
        let mut list: Vec<PluginDescriptor> =
            self.plugins.values().map(|p| p.descriptor()).collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list
    }

    pub fn get_native(&self, name: &str) -> Option<Arc<dyn CmsPlugin>> {
        self.plugins.get(name).cloned()
    }

    pub fn wasm_host(&self) -> &WasmPluginHost {
        &self.wasm_host
    }

    pub async fn boot_all(&self) -> ApiResult<()> {
        for plugin in self.plugins.values() {
            plugin.on_boot().await?;
        }
        Ok(())
    }
}

/// Zero-ambient-authority, fuel-metered WebAssembly sandbox powered by `wasmi`.
#[derive(Clone)]
pub struct WasmPluginHost {
    engine: Engine,
}

impl Default for WasmPluginHost {
    fn default() -> Self {
        Self::new()
    }
}

impl WasmPluginHost {
    pub fn new() -> Self {
        let mut cfg = Config::default();
        cfg.consume_fuel(true);
        Self {
            engine: Engine::new(&cfg),
        }
    }

    /// Validates that `bytes` is a well-formed WebAssembly module within size bounds and exports `run`.
    pub fn validate_module(&self, bytes: &[u8]) -> ApiResult<()> {
        if bytes.len() < 8 || bytes.len() > WASM_MAX_BINARY_BYTES {
            return Err(ApiError::Validation {
                message: "WASM binary must be between 8 bytes and 4 MB".into(),
                details: serde_json::json!({ "byteLength": bytes.len() }),
            });
        }
        if &bytes[0..4] != b"\0asm" {
            return Err(ApiError::Validation {
                message: "Invalid WebAssembly magic header (expected \\0asm)".into(),
                details: serde_json::json!({}),
            });
        }
        let module = Module::new(&self.engine, bytes).map_err(|e| ApiError::Validation {
            message: format!("Invalid WebAssembly module: {e}"),
            details: serde_json::json!({}),
        })?;
        let has_run = module.exports().any(|ex| ex.name() == "run");
        if !has_run {
            return Err(ApiError::Validation {
                message: "WASM plugin must export `run: (i32) -> i32`".into(),
                details: serde_json::json!({}),
            });
        }
        Ok(())
    }

    /// Executes `run(input_metric: i32) -> i32` inside an isolated `wasmi::Store` with zero host imports
    /// and a strict `250_000` instruction fuel ceiling.
    pub fn execute_sandboxed(
        &self,
        wasm_bytes: &[u8],
        input_value: i32,
    ) -> ApiResult<(i32, u64)> {
        self.validate_module(wasm_bytes)?;
        let module = Module::new(&self.engine, wasm_bytes)
            .map_err(|e| ApiError::BadRequest(format!("Compile WASM failed: {e}")))?;

        let limits = StoreLimitsBuilder::new()
            .memory_size(WASM_MAX_MEMORY_BYTES)
            .table_elements(1024)
            .instances(1)
            .memories(1)
            .tables(1)
            .build();
        let mut store = Store::new(&self.engine, limits);
        store.limiter(|lim| lim);
        store
            .set_fuel(WASM_DEFAULT_FUEL)
            .map_err(|e| ApiError::Internal(format!("Set WASM fuel failed: {e}")))?;

        let linker = Linker::<StoreLimits>::new(&self.engine);
        let instance = linker
            .instantiate(&mut store, &module)
            .map_err(|e| ApiError::BadRequest(format!("Instantiate WASM failed: {e}")))?
            .start(&mut store)
            .map_err(|e| ApiError::BadRequest(format!("WASM start trap: {e}")))?;

        let run_fn = instance
            .get_typed_func::<i32, i32>(&store, "run")
            .map_err(|e| ApiError::BadRequest(format!("Signature mismatch for `run(i32) -> i32`: {e}")))?;

        let output = run_fn.call(&mut store, input_value).map_err(|e| {
            ApiError::BadRequest(format!(
                "WASM plugin trapped during sandboxed execution (fuel/bounds): {e}"
            ))
        })?;

        let remaining = store.get_fuel().unwrap_or(0);
        let consumed = WASM_DEFAULT_FUEL.saturating_sub(remaining);
        Ok((output, consumed))
    }
}

/// Curated Marketplace catalog with pre-compiled minimal WebAssembly bytecode modules (`run(i32) -> i32`).
pub fn builtin_marketplace_catalog() -> Vec<(&'static str, &'static str, &'static str, &'static str, Vec<u8>)> {
    vec![
        (
            "wasm-reading-time",
            "1.0.0",
            "Editorial",
            "Sandboxed WASM estimator converting character length (`run(chars)`) into reading seconds.",
            // Minimal valid WASM module: `(module (func (export "run") (param i32) (result i32) local.get 0 i32.const 15 i32.div_u i32.const 1 i32.add))`
            vec![
                0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, // \0asm v1
                0x01, 0x06, 0x01, 0x60, 0x01, 0x7f, 0x01, 0x7f, // type 0: (i32) -> i32
                0x03, 0x02, 0x01, 0x00,                         // func 0 uses type 0
                0x07, 0x07, 0x01, 0x03, 0x72, 0x75, 0x6e, 0x00, 0x00, // export "run" = func 0
                0x0a, 0x0c, 0x01, 0x0a, 0x00, 0x20, 0x00, 0x41, 0x0f, 0x6e, 0x41, 0x01, 0x6a, 0x0b, // body
            ],
        ),
        (
            "wasm-word-scorer",
            "1.0.0",
            "SEO",
            "Sandboxed WASM content density scorer (`run(char_len) -> score 0..100`).",
            // `(module (func (export "run") (param i32) (result i32) local.get 0 i32.const 10 i32.div_u))`
            vec![
                0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00,
                0x01, 0x06, 0x01, 0x60, 0x01, 0x7f, 0x01, 0x7f,
                0x03, 0x02, 0x01, 0x00,
                0x07, 0x07, 0x01, 0x03, 0x72, 0x75, 0x6e, 0x00, 0x00,
                0x0a, 0x09, 0x01, 0x07, 0x00, 0x20, 0x00, 0x41, 0x0a, 0x6e, 0x0b,
            ],
        ),
        (
            "wasm-hash-tagger",
            "1.0.0",
            "Automation",
            "Sandboxed WASM deterministic shard/bucket calculator (`run(seed) -> shard_id`).",
            // `(module (func (export "run") (param i32) (result i32) local.get 0 i32.const 16 i32.rem_u))`
            vec![
                0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00,
                0x01, 0x06, 0x01, 0x60, 0x01, 0x7f, 0x01, 0x7f,
                0x03, 0x02, 0x01, 0x00,
                0x07, 0x07, 0x01, 0x03, 0x72, 0x75, 0x6e, 0x00, 0x00,
                0x0a, 0x09, 0x01, 0x07, 0x00, 0x20, 0x00, 0x41, 0x10, 0x70, 0x0b,
            ],
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wasm_host_executes_valid_module_and_traps_infinite_loops() {
        let host = WasmPluginHost::new();
        let catalog = builtin_marketplace_catalog();
        let (_, _, _, _, reading_wasm) = &catalog[0];

        let (seconds, fuel_used) = host.execute_sandboxed(reading_wasm, 300).unwrap();
        assert_eq!(seconds, 21); // 300 / 15 + 1
        assert!(fuel_used > 0);

        let (score, _) = host.execute_sandboxed(&catalog[1].4, 500).unwrap();
        assert_eq!(score, 50);

        let (shard, _) = host.execute_sandboxed(&catalog[2].4, 35).unwrap();
        assert_eq!(shard, 3);

        // Infinite loop WASM module: `(module (func (export "run") (param i32) (result i32) (loop br 0) i32.const 0))`
        let infinite_loop_wasm: &[u8] = &[
            0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00,
            0x01, 0x06, 0x01, 0x60, 0x01, 0x7f, 0x01, 0x7f,
            0x03, 0x02, 0x01, 0x00,
            0x07, 0x07, 0x01, 0x03, 0x72, 0x75, 0x6e, 0x00, 0x00,
            0x0a, 0x0b, 0x01, 0x09, 0x00, 0x03, 0x40, 0x0c, 0x00, 0x0b, 0x41, 0x00, 0x0b,
        ];
        let trapped = host.execute_sandboxed(infinite_loop_wasm, 1);
        assert!(trapped.is_err());

        // Memory-bomb WASM module requesting 256 pages (16 MiB > 4 MiB StoreLimits cap)
        let oom_wasm: &[u8] = &[
            0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00,
            0x01, 0x06, 0x01, 0x60, 0x01, 0x7f, 0x01, 0x7f,
            0x03, 0x02, 0x01, 0x00,
            0x05, 0x04, 0x01, 0x00, 0x80, 0x02,
            0x07, 0x07, 0x01, 0x03, 0x72, 0x75, 0x6e, 0x00, 0x00,
            0x0a, 0x06, 0x01, 0x04, 0x00, 0x20, 0x00, 0x0b,
        ];
        let mem_denied = host.execute_sandboxed(oom_wasm, 1);
        assert!(mem_denied.is_err());
    }
}
