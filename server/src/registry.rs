//! Module Registry — holds all loaded WASM modules and their routing info.
//!
//! Supports blue-green deployment: each module has `blue` and `green` slots.
//! Only the `active` slot serves traffic. The dashboard swaps them atomically.

use actix_web::web;
use std::collections::HashMap;
use std::sync::Arc;
use wasm_module::ModuleContext;

use super::scope;

// ---------------------------------------------------------------------------
// ModuleRegistry
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct ModuleRegistry {
    modules: HashMap<String, ModuleSlots>,
}

/// A module's blue-green deployment slots.
pub struct ModuleSlots {
    /// The currently-active slot name: `"blue"` or `"green"`.
    pub active: String,
    /// The blue slot — may be empty.
    pub blue: Option<ModuleEntry>,
    /// The green slot — may be empty.
    pub green: Option<ModuleEntry>,
}

/// A deployed version of a module.
pub struct ModuleEntry {
    pub version: (u16, u16, u16),
    pub ctx: Arc<ModuleContext>,
    /// When this entry was deployed (for dashboard display).
    pub deployed_at: String,
}

impl ModuleRegistry {
    pub fn new() -> Self {
        Self {
            modules: HashMap::new(),
        }
    }

    // -- Blue-green deployment --------------------------------------------

    /// Deploy a module into the **inactive** slot. If no slots exist yet,
    /// deploys into `blue` and marks it active.
    ///
    /// Returns `(slot_name, was_swapped)`.
    pub fn deploy(
        &mut self,
        name: impl Into<String>,
        ctx: ModuleContext,
        version: (u16, u16, u16),
    ) -> (&str, bool) {
        let name = name.into();
        let now = chrono_now();

        let entry = ModuleEntry {
            version,
            ctx: Arc::new(ctx),
            deployed_at: now,
        };

        let slots = self.modules.entry(name).or_insert_with(|| ModuleSlots {
            active: "blue".into(),
            blue: None,
            green: None,
        });

        // Deploy to the inactive slot
        let target = if slots.active == "blue" { "green" } else { "blue" };
        match target {
            "blue" => slots.blue = Some(entry),
            "green" => slots.green = Some(entry),
            _ => unreachable!(),
        }

        // Auto-activate on first deploy
        let swapped = if slots.active != target
            && slots.green.is_some()
            && slots.blue.is_some()
        {
            // Both slots are populated — perform the swap
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

    /// Swap the active slot (blue ↔ green). No-op if the inactive slot is empty.
    pub fn swap(&mut self, name: &str) -> Option<&str> {
        let slots = self.modules.get_mut(name)?;
        let new_active = if slots.active == "blue" { "green" } else { "blue" };

        // Only swap if the other slot has a module
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

    /// Remove a module entirely (both slots).
    pub fn remove(&mut self, name: &str) -> bool {
        self.modules.remove(name).is_some()
    }

    // -- Accessors --------------------------------------------------------

    /// Get the **active** context for a module (used by the router).
    pub fn active_ctx(&self, name: &str) -> Option<&Arc<ModuleContext>> {
        let slots = self.modules.get(name)?;
        match slots.active.as_str() {
            "blue" => slots.blue.as_ref().map(|e| &e.ctx),
            "green" => slots.green.as_ref().map(|e| &e.ctx),
            _ => None,
        }
    }

    /// Mount all active modules onto an Actix [`ServiceConfig`].
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

    // -- Dashboard API data -----------------------------------------------

    pub fn len(&self) -> usize {
        self.modules.len()
    }

    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }

    /// Return all modules with their blue/green status for the dashboard.
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
// Dashboard data types
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
    // Simple timestamp for the demo — no chrono dependency needed.
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
