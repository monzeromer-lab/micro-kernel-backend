//! Module Registry — holds all loaded WASM modules and their routing info.
//!
//! Supports blue-green deployment and stores module instances so exports
//! can be called by other modules via the [`ServiceRegistry`](super::services::ServiceRegistry).

use actix_web::web;
use std::collections::HashMap;
use std::sync::Arc;
use wasm_module::{ModuleContext, WasmModule};

use super::scope;

// ---------------------------------------------------------------------------
// ModuleRegistry
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct ModuleRegistry {
    modules: HashMap<String, ModuleSlots>,
}

pub struct ModuleSlots {
    pub active: String,
    pub blue: Option<ModuleEntry>,
    pub green: Option<ModuleEntry>,
}

pub struct ModuleEntry {
    pub version: (u16, u16, u16),
    pub ctx: Arc<ModuleContext>,
    pub deployed_at: String,
    /// The module instance — used to call exports via [`WasmModule::on_export_call`].
    pub module: Option<Arc<dyn WasmModule>>,
}

impl ModuleRegistry {
    pub fn new() -> Self {
        Self {
            modules: HashMap::new(),
        }
    }

    // -- Blue-green deployment --------------------------------------------

    pub fn deploy(
        &mut self,
        name: impl Into<String>,
        ctx: ModuleContext,
        version: (u16, u16, u16),
        module: Option<Arc<dyn WasmModule>>,
    ) -> (&str, bool) {
        let name = name.into();
        let now = chrono_now();

        let entry = ModuleEntry {
            version,
            ctx: Arc::new(ctx),
            deployed_at: now,
            module,
        };

        let slots = self.modules.entry(name).or_insert_with(|| ModuleSlots {
            active: "blue".into(),
            blue: None,
            green: None,
        });

        let target = if slots.active == "blue" { "green" } else { "blue" };
        match target {
            "blue" => slots.blue = Some(entry),
            "green" => slots.green = Some(entry),
            _ => unreachable!(),
        }

        let swapped = if slots.active != target
            && slots.green.is_some()
            && slots.blue.is_some()
        {
            slots.active = target.to_string();
            true
        } else if slots.blue.is_some() && slots.green.is_none() && slots.active != "blue" {
            slots.active = "blue".into();
            true
        } else if slots.green.is_some() && slots.blue.is_none() && slots.active != "green" {
            slots.active = "green".into();
            true
        } else {
            false
        };

        (target, swapped)
    }

    pub fn swap(&mut self, name: &str) -> Option<&str> {
        let slots = self.modules.get_mut(name)?;
        let new_active = if slots.active == "blue" { "green" } else { "blue" };

        let has_other = match new_active {
            "blue" => slots.blue.is_some(),
            "green" => slots.green.is_some(),
            _ => false,
        };

        if has_other {
            slots.active = new_active.to_string();
            Some(new_active)
        } else {
            None
        }
    }

    pub fn remove(&mut self, name: &str) -> bool {
        self.modules.remove(name).is_some()
    }

    // -- Accessors --------------------------------------------------------

    pub fn active_ctx(&self, name: &str) -> Option<&Arc<ModuleContext>> {
        let slots = self.modules.get(name)?;
        match slots.active.as_str() {
            "blue" => slots.blue.as_ref().map(|e| &e.ctx),
            "green" => slots.green.as_ref().map(|e| &e.ctx),
            _ => None,
        }
    }

    /// Get the active module instance (for export calls).
    pub fn active_module(&self, name: &str) -> Option<Arc<dyn WasmModule>> {
        let slots = self.modules.get(name)?;
        match slots.active.as_str() {
            "blue" => slots.blue.as_ref().and_then(|e| e.module.clone()),
            "green" => slots.green.as_ref().and_then(|e| e.module.clone()),
            _ => None,
        }
    }

    pub fn configure_all(&self, cfg: &mut web::ServiceConfig) {
        let mut names: Vec<&String> = self.modules.keys().collect();
        names.sort();

        for name in names {
            if let Some(ctx) = self.active_ctx(name) {
                cfg.service(
                    web::scope(&format!("/{}", name)).configure(|inner| {
                        scope::mount_context(inner, ctx);
                    }),
                );
            }
        }
    }

    // -- Dashboard ---------------------------------------------------------

    pub fn len(&self) -> usize {
        self.modules.len()
    }

    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }

    pub fn snapshot(&self) -> Vec<ModuleSnapshot> {
        let mut names: Vec<&String> = self.modules.keys().collect();
        names.sort();

        names
            .into_iter()
            .map(|name| {
                let slots = &self.modules[name];
                ModuleSnapshot {
                    name: name.clone(),
                    active_slot: slots.active.clone(),
                    blue: slots.blue.as_ref().map(|e| SlotSnapshot {
                        version: format!("{}.{}.{}", e.version.0, e.version.1, e.version.2),
                        deployed_at: e.deployed_at.clone(),
                    }),
                    green: slots.green.as_ref().map(|e| SlotSnapshot {
                        version: format!("{}.{}.{}", e.version.0, e.version.1, e.version.2),
                        deployed_at: e.deployed_at.clone(),
                    }),
                }
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Dashboard types
// ---------------------------------------------------------------------------

#[derive(serde::Serialize)]
pub struct ModuleSnapshot {
    pub name: String,
    pub active_slot: String,
    pub blue: Option<SlotSnapshot>,
    pub green: Option<SlotSnapshot>,
}

#[derive(serde::Serialize)]
pub struct SlotSnapshot {
    pub version: String,
    pub deployed_at: String,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn chrono_now() -> String {
    use std::time::SystemTime;
    if let Ok(dur) = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
        let secs = dur.as_secs();
        let hours = (secs / 3600) % 24;
        let mins = (secs / 60) % 60;
        let secs = secs % 60;
        format!("{:02}:{:02}:{:02}", hours, mins, secs)
    } else {
        "unknown".into()
    }
}
