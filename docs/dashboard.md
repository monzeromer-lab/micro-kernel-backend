# Dashboard

The dashboard is a web UI for managing WASM modules. Open it at:

```
http://localhost:8080/dashboard
```

## Layout

```
┌──────────────────────────────────────────────────────┐
│  ● Micro-kernel Dashboard          ⬆ Deploy  ↻ Refresh │
├──────────────────────────────────────────────────────┤
│  ┌────────┐  ┌────────┐  ┌────────┐                  │
│  │   2    │  │   2    │  │ 0h 5m  │                  │
│  │ MODULES│  │ ACTIVE │  │ UPTIME │                  │
│  └────────┘  └────────┘  └────────┘                  │
├──────────────────────────────────────────────────────┤
│  Deployed Modules                   Updated 14:22:05  │
│                                                       │
│  ┌──────────────────────────────────────────────┐     │
│  │ user  /user/*             Swap    Remove     │     │
│  │ ┌──────────────┐  ┌──────────────────────┐   │     │
│  │ │ BLUE          │  │ GREEN  ●             │   │     │
│  │ │ v1.0.0        │  │ v2.0.0  LIVE         │   │     │
│  │ │ deployed      │  │ deployed 14:22:05    │   │     │
│  │ │ 14:20:00      │  │                      │   │     │
│  │ └──────────────┘  └──────────────────────┘   │     │
│  └──────────────────────────────────────────────┘     │
│                                                       │
│  ┌──────────────────────────────────────────────┐     │
│  │ order  /order/*           Swap    Remove     │     │
│  │ ┌──────────────┐  ┌──────────────────────┐   │     │
│  │ │ BLUE  ●       │  │ GREEN                │   │     │
│  │ │ v1.0.0  LIVE  │  │ (empty)              │   │     │
│  │ └──────────────┘  └──────────────────────┘   │     │
│  └──────────────────────────────────────────────┘     │
├──────────────────────────────────────────────────────┤
│  Server Control                                       │
│  ┌──────────────────┐  ┌────────────────────────┐    │
│  │ Graceful Shutdown │  │ Force Shutdown          │    │
│  │ Finishes requests │  │ Kills immediately       │    │
│  │ [Shutdown]        │  │ [Force Shutdown]        │    │
│  └──────────────────┘  └────────────────────────┘    │
└──────────────────────────────────────────────────────┘
```

## Top Bar

| Button | Action |
|--------|--------|
| `⬆ Deploy` | Upload a `.wasm` file |
| `↻ Refresh` | Manually refresh module list |

## Module Cards

Each module shows two slots: **Blue** and **Green**.
- `● LIVE` indicator — which slot is serving traffic
- Version number + deploy timestamp
- `Swap` button — blue ↔ green (instant rollback)
- `Remove` button — delete module entirely

## Server Control

Two shutdown modes at the bottom:

| Button | What it does |
|--------|-------------|
| **Graceful Shutdown** | Stops accepting new connections, finishes in-flight requests, exits cleanly |
| **Force Shutdown** | Terminates immediately. In-flight requests are aborted. |

## Deploying a Module

### Via UI

Click `⬆ Deploy` → select `.wasm` file → auto-deploys to inactive slot → swaps if both slots full.

### Via API

```bash
curl -F module=@user.wasm http://localhost:8080/api/modules/deploy
```

### Via File Watcher

Drop `.wasm` into `./modules/`. The watcher detects it automatically.
