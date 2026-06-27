# Blue-Green Deployment

This project implements blue-green deployment **in-memory** at the module level.
No containers, no load balancers, no Kubernetes — just a single field assignment.

## The Data Structure

```rust
pub struct ModuleSlots {
    pub active: String,              // "blue" or "green"
    pub blue: Option<ModuleEntry>,    // one version
    pub green: Option<ModuleEntry>,   // another version
}

pub struct ModuleEntry {
    pub version: (u16, u16, u16),          // semver
    pub ctx: Arc<ModuleContext>,            // route definitions
    pub deployed_at: String,               // timestamp
    pub module: Option<Arc<dyn WasmModule>>, // for inter-module exports
}
```

Each module name (`"user"`, `"order"`, etc.) maps to one `ModuleSlots`.
The `active` field decides which slot's `ModuleContext` gets mounted to Actix.

## Routing: Only the Active Slot Serves Traffic

```rust
// Called every time Actix builds its routing table
pub fn configure_all(&self, cfg: &mut web::ServiceConfig) {
    for name in self.modules.keys() {
        if let Some(ctx) = self.active_ctx(name) {   // ← checks active
            cfg.service(
                web::scope(&format!("/{name}"))
                    .configure(|inner| mount_context(inner, ctx))
            );
        }
    }
}

fn active_ctx(&self, name: &str) -> Option<&Arc<ModuleContext>> {
    let slots = self.modules.get(name)?;
    match slots.active.as_str() {
        "blue"  => slots.blue.as_ref().map(|e| &e.ctx),
        "green" => slots.green.as_ref().map(|e| &e.ctx),
        _ => None,
    }
}
```

The `active` field is the **only** thing the router looks at. The inactive slot's
`ModuleContext` sits in memory, fully ready, but never mounted.

## Deploy: Write to Inactive, Then Swap

```rust
pub fn deploy(&mut self, name: &str, ctx: ModuleContext, version: (u16, u16, u16)) {
    let slots = self.modules.entry(name).or_insert(ModuleSlots {
        active: "blue".into(),
        blue: None,
        green: None,
    });

    // 1. Write to the INACTIVE slot
    let target = if slots.active == "blue" { "green" } else { "blue" };
    *slot_mut(target) = Some(ModuleEntry { version, ctx, ... });

    // 2. If both slots are now full, swap to the new one
    if slots.blue.is_some() && slots.green.is_some() {
        slots.active = target.to_string();  // ← THE SWAP
    }
}
```

The swap is **one line**: `slots.active = target.to_string()`.
No copying routes. No recompiling WASM. No mutating shared state that other
threads are reading (the `&mut self` borrow guarantees exclusive access).

## Swap: Manual Rollback

```rust
pub fn swap(&mut self, name: &str) -> Option<&str> {
    let slots = self.modules.get_mut(name)?;

    // Pick the opposite slot
    let new_active = if slots.active == "blue" { "green" } else { "blue" };

    // Only swap if there's something in the inactive slot
    if slot_is_some(new_active) {
        slots.active = new_active.to_string();  // ← THE SWAP
        Some(new_active)
    } else {
        None
    }
}
```

## Lifecycle Walkthrough

### Initial State

```
Registry: (empty)
```

### Deploy v1.0.0

```
1. ModuleSlots created: active="blue", blue=None, green=None
2. target = "green" (inactive since active is "blue")
3. green = Some(v1.0.0)
4. Check: blue is None → no swap yet
5. But: green is Some && blue is None → auto-activate: active = "green"

Result:
  BLUE  (empty)
  GREEN v1.0.0  ● LIVE
```

### Deploy v2.0.0

```
1. active is "green" → target = "blue"
2. blue = Some(v2.0.0)  [overwrites if was Some]
3. Check: blue=Some && green=Some && active!=target → SWAP!
4. active = "blue"

Result:
  BLUE  v2.0.0  ● LIVE
  GREEN v1.0.0
```

### Deploy v3.0.0

```
1. active is "blue" → target = "green"
2. green = Some(v3.0.0)  [OVERWRITES v1.0.0!]
3. Check: both full → SWAP!
4. active = "green"

Result:
  BLUE  v2.0.0
  GREEN v3.0.0  ● LIVE
```

**Note**: v1.0.0 was overwritten in step 2. Blue-green keeps **two** versions,
not a full history. For full version history, you'd store a log separately.

### Manual Swap (Rollback)

```bash
curl -X POST http://localhost:8080/api/modules/user/swap
```

```
Before:  BLUE  v2.0.0            After:  BLUE  v2.0.0  ● LIVE
         GREEN v3.0.0  ● LIVE            GREEN v3.0.0
```

One HTTP request. One field assignment. Instant rollback.

## Why This Works for a Tech Talk Demo

| Aspect | How it's handled |
|--------|-----------------|
| **Zero downtime** | Swap is a single field write — no restart, no recompile |
| **Instant rollback** | Previous version is still in memory in the other slot |
| **Visibility** | Dashboard shows both slots side-by-side with version + timestamp |
| **Simplicity** | The entire mechanism is ~60 lines of Rust in `registry.rs` |
| **No external deps** | No Redis, no load balancer, no container orchestration |

## What It Doesn't Do (yet)

- **Connection draining**: In-flight requests to the old version complete, but new
  requests go to the new version immediately. This is fine for stateless APIs.
- **Version history**: Only two versions at a time. Older versions are overwritten.
- **Canary deployments**: No traffic splitting. It's all-or-nothing per module.
- **Persistence**: Everything is in-memory. Restarting the server loses all modules.
  (For the demo, this is fine — modules are re-loaded from the filesystem.)
