# Dashboard

The dashboard is a web UI for managing WASM modules. Open it at:

```
http://localhost:8080/dashboard
```

## What You See

```
┌──────────────────────────────────────────────────────┐
│  ⚙️ Micro-kernel Dashboard        ⬆ Deploy  🔄 Refresh │
├──────────────────────────────────────────────────────┤
│  ┌────────┐  ┌────────┐  ┌────────┐                  │
│  │   0    │  │   0    │  │ 0h 2m  │                  │
│  │ TOTAL  │  │ ACTIVE │  │ UPTIME │                  │
│  └────────┘  └────────┘  └────────┘                  │
├──────────────────────────────────────────────────────┤
│  📦 Deployed Modules          Last updated: 14:22:05  │
│                                                       │
│  ┌──────────────────────────────────────────────┐     │
│  │ 📁 user  /user/*          🔀 Swap  🗑 Remove │     │
│  │ ┌──────────────┐  ┌──────────────────────┐   │     │
│  │ │ BLUE          │  │ GREEN  ● LIVE        │   │     │
│  │ │ v1.0.0        │  │ v2.0.0               │   │     │
│  │ │ deployed      │  │ deployed 14:22:05    │   │     │
│  │ │ 14:20:00      │  │            ACTIVE    │   │     │
│  │ └──────────────┘  └──────────────────────┘   │     │
│  └──────────────────────────────────────────────┘     │
└──────────────────────────────────────────────────────┘
```

### Top Bar

| Element | Action |
|---------|--------|
| `⬆ Deploy Module` | Opens file picker to upload a `.wasm` file |
| `🔄 Refresh` | Manually refresh the module list |

### Status Bar

- **Total Modules** — how many module names are registered
- **Active Deployments** — how many modules have at least one slot filled
- **Uptime** — how long the dashboard has been open

### Module Cards

Each module shows two slots side-by-side:

- **BLUE** — one deployment slot
- **GREEN** — the other deployment slot
- `● LIVE` — indicates which slot is currently serving traffic
- **ACTIVE** / **STANDBY** badge — visual indicator
- Version number and deploy timestamp

### Actions per Module

| Button | What it does |
|--------|-------------|
| `🔀 Swap` | Swap blue ↔ green (instant rollback/release) |
| `🗑 Remove` | Delete the module entirely (both slots gone) |

The **Swap** button is disabled when only one slot is populated (nothing to swap to).

## Deploying a Module

### Via the UI

1. Click `⬆ Deploy Module`
2. Select a `.wasm` file from your filesystem
3. The module is deployed to the **inactive** slot
4. If both slots are now full, it **auto-swaps** — the new version goes live

### Via the API

```bash
curl -F module=@user.wasm http://localhost:8080/api/modules/deploy
```

### Via the File Watcher

Drop a `.wasm` file into the `./modules/` directory. The watcher detects it
automatically (auto-deploy is a TODO placeholder in the current demo).

## Blue-Green Swapping

Click `🔀 Swap` to instantly switch which version is live:

```
Before swap:
  BLUE  v1.0.0  (standby)
  GREEN v2.0.0  ● LIVE

After swap:
  BLUE  v1.0.0  ● LIVE   ← rolled back!
  GREEN v2.0.0  (standby)
```

No recompilation. No restart. One field assignment in memory.

See [Blue-Green Deployment](blue-green.md) for the full mechanism.
