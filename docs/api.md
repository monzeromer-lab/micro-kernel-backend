# Dashboard API Reference

Base URL: `http://localhost:8080`

All responses are JSON. The API is consumed by the dashboard UI but can also be
used directly with `curl` or any HTTP client.

---

## `GET /api/modules`

List all registered modules with their blue-green slot status.

**Response** `200 OK`

```json
[
  {
    "name": "user",
    "active_slot": "green",
    "blue": {
      "version": "1.0.0",
      "deployed_at": "14:20:00"
    },
    "green": {
      "version": "2.0.0",
      "deployed_at": "14:22:05"
    }
  }
]
```

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Module name (also the URL prefix: `/user/*`) |
| `active_slot` | string | `"blue"` or `"green"` — which slot serves traffic |
| `blue` | object\|null | Blue slot entry (null if empty) |
| `green` | object\|null | Green slot entry (null if empty) |
| `blue.version` | string | Semantic version (`"1.2.3"`) |
| `blue.deployed_at` | string | Deployment timestamp |

Empty registry:

```json
[]
```

---

## `POST /api/modules/deploy`

Deploy a new module version. Accepts multipart form data.

**Request**

```
Content-Type: multipart/form-data

Field: module = <binary .wasm file>
```

**Response** `200 OK`

```json
{
  "message": "Deploy endpoint ready — upload a .wasm file via multipart form (demo placeholder)",
  "form_field": "module",
  "example": "curl -F module=@user.wasm http://localhost:8080/api/modules/deploy"
}
```

> **Note**: In the current demo, this endpoint is a placeholder. The real
> implementation will compile the uploaded `.wasm` file, instantiate it, call
> `WasmModule::register()`, and deploy to the inactive slot.


**curl example**

```bash
curl -F module=@user.wasm http://localhost:8080/api/modules/deploy
```

---

## `POST /api/modules/{name}/swap`

Swap the active slot (blue ↔ green). Instant, no downtime.

**Path parameter**

| Param | Type | Description |
|-------|------|-------------|
| `name` | string | Module name to swap |

**Response** `200 OK`

```json
{
  "swapped": "user",
  "active": "green"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `swapped` | string | Module name that was swapped |
| `active` | string | The new active slot (`"blue"` or `"green"`) |

**Error** `400 Bad Request`

```json
{
  "error": "cannot swap — inactive slot is empty"
}
```

Returned when only one slot has a deployed version (nothing to swap to).

**curl example**

```bash
curl -X POST http://localhost:8080/api/modules/user/swap
```

---

## `DELETE /api/modules/{name}`

Remove a module entirely. Both blue and green slots are deleted.

**Path parameter**

| Param | Type | Description |
|-------|------|-------------|
| `name` | string | Module name to remove |

**Response** `200 OK`

```json
{
  "removed": "user"
}
```

**Error** `404 Not Found`

```json
{
  "error": "not found"
}
```

**curl example**

```bash
curl -X DELETE http://localhost:8080/api/modules/user
```
