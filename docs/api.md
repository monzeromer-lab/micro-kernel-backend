# Dashboard API Reference

Base URL: `http://localhost:8080`

All responses are JSON.

---

## Modules

### `GET /api/modules`

List all modules with blue-green slot status.

**Response** `200 OK`

```json
[
  {
    "name": "user",
    "active_slot": "green",
    "blue": { "version": "1.0.0", "deployed_at": "14:20:00" },
    "green": { "version": "2.0.0", "deployed_at": "14:22:05" }
  }
]
```

Empty: `[]`

---

### `POST /api/modules/deploy`

Deploy a module. Accepts multipart form data (field `module` = binary `.wasm`).

> Currently a placeholder — the real implementation will compile and register.

```bash
curl -F module=@user.wasm http://localhost:8080/api/modules/deploy
```

---

### `POST /api/modules/{name}/swap`

Swap active slot (blue ↔ green). Instant, zero-downtime.

```bash
curl -X POST http://localhost:8080/api/modules/user/swap
```

**Response** `200 OK`

```json
{ "swapped": "user", "active": "green" }
```

**Error** `400`

```json
{ "error": "cannot swap — inactive slot is empty" }
```

---

### `DELETE /api/modules/{name}`

Remove a module entirely (both slots).

```bash
curl -X DELETE http://localhost:8080/api/modules/user
```

**Response** `200 OK`

```json
{ "removed": "user" }
```

**Error** `404`

```json
{ "error": "not found" }
```

---

## Shutdown

### `POST /api/shutdown/graceful`

Stop accepting new connections, finish in-flight requests, then exit cleanly.

```bash
curl -X POST http://localhost:8080/api/shutdown/graceful
```

**Response** `200 OK`

```json
{ "shutdown": "graceful", "message": "Server shutting down — finishing in-flight requests before exit." }
```

---

### `POST /api/shutdown/force`

Terminate immediately. In-flight requests are aborted.

```bash
curl -X POST http://localhost:8080/api/shutdown/force
```

**Response** `200 OK`

```json
{ "shutdown": "force", "message": "Server killed immediately — in-flight requests were aborted." }
```

---

## Dashboard Page

### `GET /dashboard`

Returns the dashboard HTML page.

```bash
open http://localhost:8080/dashboard
```
