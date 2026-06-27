# Micro-kernel Architecture — Tech Talk Demo

A **micro-kernel web backend** where the server core (the "kernel") is minimal and all
business logic lives in dynamically-loaded WebAssembly modules.

## Quick Start

```bash
# Set up databases (macOS Homebrew)
brew services start postgresql@16 mysql redis

# Create test data
createdb testdb
psql -d testdb -c "CREATE TABLE users (id SERIAL PRIMARY KEY, name TEXT); INSERT INTO users (name) VALUES ('Alice'),('Bob'),('Charlie');"
mysql -u root -e "CREATE DATABASE dabdoob; CREATE USER 'ec-user'@'localhost' IDENTIFIED BY 'password'; GRANT ALL ON dabdoob.* TO 'ec-user'@'localhost';"
mysql -u ec-user -ppassword dabdoob -e "CREATE TABLE users (id INT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(100)); INSERT INTO users (name) VALUES ('Alice'),('Bob'),('Charlie');"

# Start the server with real credentials
DATABASE_URL="postgres://localhost/testdb" \
MYSQL_URL="mysql://ec-user:password@localhost/dabdoob" \
REDIS_URL="redis://127.0.0.1:6379" \
S3_ENDPOINT="https://fra1.digitaloceanspaces.com" \
S3_KEY="YOUR_KEY" S3_SECRET="YOUR_SECRET" \
cargo run

# Open the dashboard
open http://localhost:8080/dashboard
```

Without env vars, the server uses **echo fallback providers** — works without any
database running. Set the env vars to connect to real services.

## Endpoints

| Path | What |
|------|------|
| `/dashboard` | Blue-green deployment dashboard |
| `/wasm/health` | Health check |
| `/user/list` | Postgres query (real sqlx pool or echo fallback) |
| `/user/cache` | Redis get (real redis-rs or echo fallback) |
| `/user/files` | S3 get (real ureq or echo fallback) |
| `/user/from-order` | Inter-module: User → Order |
| `/order/call-user` | Inter-module: Order → User |

## Environment Variables

| Variable | Service | Default |
|----------|---------|---------|
| `DATABASE_URL` | Postgres | (empty → echo fallback) |
| `MYSQL_URL` | MySQL | (empty → echo fallback) |
| `REDIS_URL` | Redis | `redis://127.0.0.1:6379` |
| `S3_ENDPOINT` | S3 | `http://localhost:9000` (MinIO) |
| `S3_KEY` | S3 | (empty → echo fallback) |
| `S3_SECRET` | S3 | (empty → echo fallback) |
| `S3_REGION` | S3 | `us-east-1` |

All providers fall back to echo implementations when credentials are missing
or connections fail — the server always starts.

## Testing

```bash
# Unit + integration tests (56 total)
cargo test

# Compile the WASM test module first
cargo build -p test-module --target wasm32-unknown-unknown

# Run WASM integration tests
cargo test --test wasm_integration
```

| Layer | Tests | What |
|-------|-------|------|
| `wasm-module` | 33 | SDK — traits, handlers, typed handles, edge cases |
| `wasm-server` unit | 17 | Registry, services, exports, watcher |
| WASM integration | 3 | Real `.wasm` module: compile → init → call Postgres/Redis/S3/HTTP |
| Doc tests | 3 | Inline code examples |

## Project Layout

```
wasm/
├── docs/                    # Documentation (8 files)
├── modules/                 # Drop .wasm files here
├── test-module/             # WASM test module (compiles to wasm32)
├── wasm-module/             # Module SDK crate (publishable to crates.io)
└── server/                  # Micro-kernel runtime
    ├── static/dashboard.html
    └── src/
        ├── main.rs          # Entry point + demo modules
        ├── providers/       # Real implementations (sqlx, redis-rs, ureq)
        │   ├── postgres.rs  #   PostgresProvider — OS-thread async execution
        │   ├── mysql.rs     #   MySqlProvider — OS-thread async execution
        │   ├── redis_provider.rs # RedisProvider — sync Mutex<Connection>
        │   ├── s3.rs        #   S3Provider — sync ureq
        │   └── http_client.rs   # HttpProvider — sync ureq
        ├── registry.rs      # Blue-green deployment slots
        ├── services.rs      # ServiceRegistry + ServiceProvider trait
        ├── watcher.rs       # File watcher (notify)
        └── engine/          # wasmtime engine + host functions
```

## Key Features

| Feature | For the talk |
|---------|-------------|
| **Typed service handles** | `pg.query("SELECT ...")`, `redis.get("key")`, `s3.get("b","k")`, `http.get("url")` |
| **Real providers** | `sqlx` (PG/MySQL with OS-thread async), `redis-rs`, `ureq` (S3/HTTP sync) |
| **Echo fallback** | Server runs without any database — all providers degrade gracefully |
| **Blue-green deployment** | Dashboard — deploy v2, Swap, instant rollback |
| **Inter-module calls** | `call_module("user", "get_name", args)` with typed `FromModuleBytes` |
| **WASM integration** | Real `.wasm` module calls Postgres/Redis/S3/HTTP through host functions |
| **Graceful shutdown** | Dashboard → Server Control |

## Further Reading

- [Architecture](architecture.md) — kernel internals, data flow, OS-thread execution
- [Creating Modules](modules.md) — full API reference for module authors
- [External Services](services.md) — provider guide, config, adding new ones
- [Dashboard](dashboard.md) — UI guide
- [API Reference](api.md) — REST endpoints + shutdown
- [Blue-Green](blue-green.md) — deployment mechanism
- [Summary](SUMMARY.md) — team overview
