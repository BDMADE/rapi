use async_trait::async_trait;
use cms_core::ApiResult;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

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
}

#[derive(Default, Clone)]
pub struct PluginRegistry {
    plugins: Vec<Arc<dyn CmsPlugin>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
        }
    }

    pub fn register(&mut self, plugin: Arc<dyn CmsPlugin>) {
        self.plugins.push(plugin);
    }

    pub fn descriptors(&self) -> Vec<PluginDescriptor> {
        self.plugins.iter().map(|p| p.descriptor()).collect()
    }

    pub async fn boot_all(&self) -> ApiResult<()> {
        for plugin in &self.plugins {
            plugin.on_boot().await?;
        }
        Ok(())
    }
}
