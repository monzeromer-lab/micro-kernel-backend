use std::borrow::Cow;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// WasmModule — the contract every module must implement
// ---------------------------------------------------------------------------

pub trait WasmModule: Send + Sync {
    /// Called once when the module is loaded. Register routes, middleware,
    /// guards, exports, and nested scopes using the provided [`ModuleContext`].
    ///
    /// The context also provides [`call_service`](ModuleContext::call_service)
    /// and [`call_module`](ModuleContext::call_module) for inter-module and
    /// external-service communication.
    fn register(&self, ctx: &mut ModuleContext);

    /// Declare the runtime properties this module needs.
    fn properties(&self) -> ModuleProperties {
        ModuleProperties::default()
    }

    /// Semantic version — used for blue-green deployments.
    fn version(&self) -> (u16, u16, u16) {
        (0, 1, 0)
    }

    /// Called by the kernel when **another module** invokes one of this
    /// module's exported functions (declared via [`ModuleContext::export`]).
    ///
    /// Return the response bytes. Return empty vec for unknown functions.
    fn on_export_call(&self, _function: &str, _args: &[u8]) -> Vec<u8> {
        vec![]
    }
}

// ---------------------------------------------------------------------------
// ModuleProperties
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ModuleProperties {
    /// Minimum linear memory pages (64 KiB each by default).
    pub memory_pages: u32,
    /// Maximum memory pages (None = unbounded).
    pub max_memory_pages: Option<u32>,
    /// Whether the module uses 64-bit memories.
    pub memory64: bool,
    /// Whether fuel-based yielding is needed.
    pub consume_fuel: bool,
    /// Maximum Wasm stack in bytes (None = host default, 512 KiB).
    pub max_wasm_stack: Option<usize>,

    /// External services this module needs (database, HTTP, Redis, etc.).
    pub required_services: Vec<ServiceRequirement>,
    /// Other modules this module depends on (loaded first, exports available).
    pub required_modules: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ServiceRequirement {
    /// Type of service.
    pub kind: ServiceKind,
    /// A unique identifier within that service kind, e.g. `"main_db"`.
    pub identifier: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceKind {
    Postgres,
    Http,
    Redis,
    MySql,
    S3,
}

impl Default for ModuleProperties {
    fn default() -> Self {
        Self {
            memory_pages: 1,
            max_memory_pages: None,
            memory64: false,
            consume_fuel: false,
            max_wasm_stack: None,
            required_services: Vec::new(),
            required_modules: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Response
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn ok(body: impl Into<Vec<u8>>) -> Self {
        Self {
            status: 200,
            headers: vec![("content-type".into(), "text/plain; charset=utf-8".into())],
            body: body.into(),
        }
    }

    pub fn json(body: impl Into<Vec<u8>>) -> Self {
        Self {
            status: 200,
            headers: vec![("content-type".into(), "application/json".into())],
            body: body.into(),
        }
    }

    pub fn created(body: impl Into<Vec<u8>>) -> Self {
        Self {
            status: 201,
            headers: vec![("content-type".into(), "text/plain; charset=utf-8".into())],
            body: body.into(),
        }
    }

    pub fn bad_request(body: impl Into<Vec<u8>>) -> Self {
        Self {
            status: 400,
            headers: vec![("content-type".into(), "text/plain; charset=utf-8".into())],
            body: body.into(),
        }
    }

    pub fn not_found() -> Self {
        Self {
            status: 404,
            headers: vec![("content-type".into(), "text/plain; charset=utf-8".into())],
            body: b"not found".to_vec(),
        }
    }

    pub fn internal_error(body: impl Into<Vec<u8>>) -> Self {
        Self {
            status: 500,
            headers: vec![("content-type".into(), "text/plain; charset=utf-8".into())],
            body: body.into(),
        }
    }
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

pub trait Handler: Send + Sync + 'static {
    fn call(&self) -> Response;
}

impl<F, R> Handler for F
where
    F: Fn() -> R + Send + Sync + 'static,
    R: Into<Response>,
{
    fn call(&self) -> Response {
        self().into()
    }
}

impl From<&str> for Response {
    fn from(s: &str) -> Self {
        Response::ok(s.as_bytes().to_vec())
    }
}

impl From<String> for Response {
    fn from(s: String) -> Self {
        Response::ok(s.into_bytes())
    }
}

// ---------------------------------------------------------------------------
// FromModuleBytes — typed inter-module / service responses
// ---------------------------------------------------------------------------

/// Trait for types that can be parsed from raw module/service call responses.
///
/// # Built-in impls (zero deps)
///
/// | Type | How it parses |
/// |------|---------------|
/// | `Vec<u8>` | Identity (no parsing) |
/// | `String` | UTF-8 decode |
/// | `i32`, `u32`, `i64`, `u64`, `f64`, `bool` | Parses from UTF-8 string |
///
/// # JSON support (enable `json` feature)
///
/// With `features = ["json"]`, any type implementing `serde::Deserialize`
/// gets a blanket impl.  The raw bytes are parsed as JSON.
///
/// # Custom impl
///
/// ```rust,ignore
/// struct MyType { value: i32 }
/// impl FromModuleBytes for MyType {
///     fn from_module_bytes(bytes: &[u8]) -> Result<Self, String> {
///         // your parsing logic here
///     }
/// }
/// ```
pub trait FromModuleBytes: Sized {
    fn from_module_bytes(bytes: &[u8]) -> Result<Self, String>;
}

// -- Built-in impls (no serde needed) ---------------------------------------

impl FromModuleBytes for Vec<u8> {
    fn from_module_bytes(bytes: &[u8]) -> Result<Self, String> {
        Ok(bytes.to_vec())
    }
}

impl FromModuleBytes for String {
    fn from_module_bytes(bytes: &[u8]) -> Result<Self, String> {
        String::from_utf8(bytes.to_vec()).map_err(|e| e.to_string())
    }
}

macro_rules! impl_from_bytes_parse {
    ($ty:ty) => {
        impl FromModuleBytes for $ty {
            fn from_module_bytes(bytes: &[u8]) -> Result<Self, String> {
                let s = String::from_utf8(bytes.to_vec()).map_err(|e| e.to_string())?;
                s.trim().parse::<$ty>().map_err(|e| e.to_string())
            }
        }
    };
}

impl_from_bytes_parse!(i32);
impl_from_bytes_parse!(u32);
impl_from_bytes_parse!(i64);
impl_from_bytes_parse!(u64);
impl_from_bytes_parse!(f64);
impl_from_bytes_parse!(bool);

// -- JSON blanket impl (behind "json" feature) -----------------------------

#[cfg(feature = "json")]
impl<T> FromModuleBytes for T
where
    T: serde::de::DeserializeOwned,
{
    fn from_module_bytes(bytes: &[u8]) -> Result<Self, String> {
        serde_json::from_slice(bytes).map_err(|e| e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Typed Service Handles — ergonomic, method-based access to external services
// ---------------------------------------------------------------------------

/// Typed handle for Postgres — modules call `.query()` / `.execute()`
/// instead of raw `call_service("postgres", ...)`.
pub trait PostgresHandle: Send + Sync {
    /// Run a SELECT-style query. Returns JSON rows as a String.
    fn query(&self, sql: &str) -> Result<String, String>;
    /// Run INSERT / UPDATE / DELETE. Returns rows-affected count.
    fn execute(&self, sql: &str) -> Result<u64, String>;
    /// Run a parameterised query ($1, $2, ...).
    fn query_with(&self, sql: &str, params: &[&str]) -> Result<String, String>;
}

/// Typed handle for Redis — modules call `.get()` / `.set()` / etc.
pub trait RedisHandle: Send + Sync {
    /// Get a key. `Ok(None)` means the key doesn't exist.
    fn get(&self, key: &str) -> Result<Option<String>, String>;
    /// Set a key with optional TTL in seconds.
    fn set(&self, key: &str, value: &str, ttl_seconds: Option<u64>) -> Result<(), String>;
    /// Delete one or more keys. Returns the count of keys deleted.
    fn del(&self, keys: &[&str]) -> Result<u64, String>;
    /// Increment a key by 1 (or by `amount`). Returns the new value.
    fn incr(&self, key: &str, amount: Option<i64>) -> Result<i64, String>;
    /// Check if a key exists.
    fn exists(&self, key: &str) -> Result<bool, String>;
}

/// Typed handle for MySQL — same shape as Postgres, separate trait for clarity.
pub trait MySqlHandle: Send + Sync {
    fn query(&self, sql: &str) -> Result<String, String>;
    fn execute(&self, sql: &str) -> Result<u64, String>;
    fn query_with(&self, sql: &str, params: &[&str]) -> Result<String, String>;
}

/// Typed handle for S3-compatible object storage.
pub trait S3Handle: Send + Sync {
    /// Put an object into a bucket. Returns the object key on success.
    fn put(&self, bucket: &str, key: &str, data: &[u8]) -> Result<String, String>;
    /// Get an object from a bucket. Returns the raw bytes.
    fn get(&self, bucket: &str, key: &str) -> Result<Vec<u8>, String>;
    /// Delete an object. Returns true if it existed.
    fn delete(&self, bucket: &str, key: &str) -> Result<bool, String>;
    /// List objects in a bucket with an optional prefix filter.
    fn list(&self, bucket: &str, prefix: Option<&str>) -> Result<String, String>;
}

/// Typed handle for HTTP client.
pub trait HttpHandle: Send + Sync {
    /// GET a URL. Returns the response body.
    fn get(&self, url: &str) -> Result<String, String>;
    /// POST to a URL with a body. Returns the response body.
    fn post(&self, url: &str, body: &str) -> Result<String, String>;
    /// PUT to a URL with a body.
    fn put(&self, url: &str, body: &str) -> Result<String, String>;
    /// DELETE a URL.
    fn delete(&self, url: &str) -> Result<String, String>;
}

// ---------------------------------------------------------------------------
// Middleware
// ---------------------------------------------------------------------------

pub trait Middleware: Send + Sync + 'static {
    fn name(&self) -> Cow<'static, str>;
    fn before(&self) -> bool { true }
    fn after(&self) -> bool { true }
}

// ---------------------------------------------------------------------------
// Guard
// ---------------------------------------------------------------------------

pub trait Guard: Send + Sync + 'static {
    fn name(&self) -> Cow<'static, str>;
    fn check(&self) -> bool;
}

// ---------------------------------------------------------------------------
// ModuleContext — the registration API, now with inter-module + service calls
// ---------------------------------------------------------------------------

/// Callback: call an external service (database, HTTP, Redis, etc.).
pub type ServiceCallFn = dyn Fn(&str, &str, &[u8]) -> Vec<u8> + Send + Sync;
/// Callback: call a function exported by another module.
pub type ModuleCallFn = dyn Fn(&str, &str, &[u8]) -> Vec<u8> + Send + Sync;

pub struct ModuleContext {
    routes: Vec<RouteDef>,
    scopes: Vec<ScopeDef>,
    middleware: Vec<Box<dyn Middleware>>,
    guards: Vec<Box<dyn Guard>>,

    /// Functions this module exports for other modules to call.
    exports: Vec<String>,

    /// Set by the host before `register()` — call an external service (raw).
    pub call_service: Option<Arc<ServiceCallFn>>,
    /// Set by the host before `register()` — call another module's export (raw).
    pub call_module: Option<Arc<ModuleCallFn>>,

    // ── Typed service handles (set by host) ────────────────────────────────

    /// Typed Postgres access. Set by host if module declares
    /// `required_services` with `ServiceKind::Postgres`.
    pub postgres: Option<Arc<dyn PostgresHandle>>,
    /// Typed Redis access.
    pub redis: Option<Arc<dyn RedisHandle>>,
    /// Typed MySQL access.
    pub mysql: Option<Arc<dyn MySqlHandle>>,
    /// Typed S3 access.
    pub s3: Option<Arc<dyn S3Handle>>,
    /// Typed HTTP client access.
    pub http: Option<Arc<dyn HttpHandle>>,
}

pub struct RouteDef {
    pub method: Method,
    pub path: String,
    pub handler: Box<dyn Handler>,
    pub guards: Vec<Box<dyn Guard>>,
}

pub struct ScopeDef {
    pub prefix: String,
    pub context: ModuleContext,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Put,
    Delete,
    Patch,
}

impl ModuleContext {
    pub fn new() -> Self {
        Self {
            routes: Vec::new(),
            scopes: Vec::new(),
            middleware: Vec::new(),
            guards: Vec::new(),
            exports: Vec::new(),
            call_service: None,
            call_module: None,
            postgres: None,
            redis: None,
            mysql: None,
            s3: None,
            http: None,
        }
    }

    // -- Route registration -------------------------------------------------

    pub fn get(&mut self, path: impl Into<String>, handler: impl Handler) -> &mut Self {
        self.routes.push(RouteDef {
            method: Method::Get,
            path: path.into(),
            handler: Box::new(handler),
            guards: Vec::new(),
        });
        self
    }

    pub fn post(&mut self, path: impl Into<String>, handler: impl Handler) -> &mut Self {
        self.routes.push(RouteDef {
            method: Method::Post,
            path: path.into(),
            handler: Box::new(handler),
            guards: Vec::new(),
        });
        self
    }

    pub fn put(&mut self, path: impl Into<String>, handler: impl Handler) -> &mut Self {
        self.routes.push(RouteDef {
            method: Method::Put,
            path: path.into(),
            handler: Box::new(handler),
            guards: Vec::new(),
        });
        self
    }

    pub fn delete(&mut self, path: impl Into<String>, handler: impl Handler) -> &mut Self {
        self.routes.push(RouteDef {
            method: Method::Delete,
            path: path.into(),
            handler: Box::new(handler),
            guards: Vec::new(),
        });
        self
    }

    pub fn patch(&mut self, path: impl Into<String>, handler: impl Handler) -> &mut Self {
        self.routes.push(RouteDef {
            method: Method::Patch,
            path: path.into(),
            handler: Box::new(handler),
            guards: Vec::new(),
        });
        self
    }

    // -- Nested scopes ------------------------------------------------------

    pub fn scope(
        &mut self,
        prefix: impl Into<String>,
        f: impl FnOnce(&mut ModuleContext),
    ) -> &mut Self {
        let mut sub = ModuleContext::new();
        // Propagate callbacks and handles to nested scope
        sub.call_service = self.call_service.clone();
        sub.call_module = self.call_module.clone();
        sub.postgres = self.postgres.clone();
        sub.redis = self.redis.clone();
        sub.mysql = self.mysql.clone();
        sub.s3 = self.s3.clone();
        sub.http = self.http.clone();
        f(&mut sub);
        self.scopes.push(ScopeDef {
            prefix: prefix.into(),
            context: sub,
        });
        self
    }

    // -- Inter-module exports -----------------------------------------------

    /// Declare a named function that other modules can call via `call_module`.
    ///
    /// The actual handler is implemented in [`WasmModule::on_export_call`].
    pub fn export(&mut self, name: impl Into<String>) -> &mut Self {
        self.exports.push(name.into());
        self
    }

    // -- Typed inter-module & service calls ----------------------------------

    /// Call another module's exported function and parse the response
    /// into the requested type `T` via [`FromModuleBytes`].
    ///
    /// ```rust,ignore
    /// let name: String = ctx.call_module_typed("user", "get_name", b"{}")?;
    /// let count: i32  = ctx.call_module_typed("order", "count", b"{}")?;
    ///
    /// // With the `json` feature enabled:
    /// #[derive(Deserialize)]
    /// struct User { id: u32, name: String }
    /// let user: User = ctx.call_module_typed("user", "get_user", b"{}")?;
    /// ```
    pub fn call_module_typed<T: FromModuleBytes>(
        &self,
        module: &str,
        function: &str,
        args: &[u8],
    ) -> Result<T, String> {
        let f = self
            .call_module
            .as_ref()
            .ok_or_else(|| "call_module callback not set by host".to_string())?;
        let bytes = f(module, function, args);
        T::from_module_bytes(&bytes)
    }

    /// Call an external service and parse the response into type `T`
    /// via [`FromModuleBytes`].
    ///
    /// ```rust,ignore
    /// let rows: String = ctx.call_service_typed("postgres", "main_db", b"SELECT 1")?;
    ///
    /// // With the `json` feature:
    /// #[derive(Deserialize)]
    /// struct QueryResult { rows: Vec<Row> }
    /// let result: QueryResult = ctx.call_service_typed("postgres", "main_db", sql)?;
    /// ```
    pub fn call_service_typed<T: FromModuleBytes>(
        &self,
        kind: &str,
        identifier: &str,
        payload: &[u8],
    ) -> Result<T, String> {
        let f = self
            .call_service
            .as_ref()
            .ok_or_else(|| "call_service callback not set by host".to_string())?;
        let bytes = f(kind, identifier, payload);
        T::from_module_bytes(&bytes)
    }

    // -- Middleware & Guards ------------------------------------------------

    pub fn middleware(&mut self, mw: impl Middleware) -> &mut Self {
        self.middleware.push(Box::new(mw));
        self
    }

    pub fn guard(&mut self, guard: impl Guard) -> &mut Self {
        self.guards.push(Box::new(guard));
        self
    }

    // -- Accessors (used by the host) ---------------------------------------

    pub fn routes(&self) -> impl Iterator<Item = &RouteDef> {
        self.routes.iter()
    }

    pub fn scopes(&self) -> impl Iterator<Item = &ScopeDef> {
        self.scopes.iter()
    }

    pub fn exports(&self) -> impl Iterator<Item = &String> {
        self.exports.iter()
    }

    pub fn middleware_entries(&self) -> impl Iterator<Item = &dyn Middleware> {
        self.middleware.iter().map(|m| m.as_ref())
    }

    pub fn guard_entries(&self) -> impl Iterator<Item = &dyn Guard> {
        self.guards.iter().map(|g| g.as_ref())
    }
}

impl Default for ModuleContext {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    struct TestModule;

    impl WasmModule for TestModule {
        fn register(&self, ctx: &mut ModuleContext) {
            ctx.export("get_name")
               .get("/", || "hello")
               .scope("/admin", |admin| {
                   admin.get("/dashboard", || "admin dashboard");
               });
        }

        fn properties(&self) -> ModuleProperties {
            ModuleProperties {
                memory_pages: 2,
                required_services: vec![ServiceRequirement {
                    kind: ServiceKind::Postgres,
                    identifier: "main_db".into(),
                }],
                required_modules: vec!["order".into()],
                ..Default::default()
            }
        }

        fn version(&self) -> (u16, u16, u16) {
            (1, 2, 3)
        }

        fn on_export_call(&self, function: &str, _args: &[u8]) -> Vec<u8> {
            match function {
                "get_name" => b"TestModule".to_vec(),
                _ => vec![],
            }
        }
    }

    #[test]
    fn test_module_registers_routes_and_exports() {
        let mut ctx = ModuleContext::new();
        TestModule.register(&mut ctx);

        assert_eq!(ctx.routes.len(), 1);
        assert_eq!(ctx.scopes.len(), 1);
        assert_eq!(ctx.exports().count(), 1);

        let exports: Vec<&String> = ctx.exports().collect();
        assert_eq!(exports[0], "get_name");
    }

    #[test]
    fn test_on_export_call() {
        let m = TestModule;
        assert_eq!(m.on_export_call("get_name", &[]), b"TestModule".to_vec());
        assert_eq!(m.on_export_call("unknown", &[]), vec![]);
    }

    #[test]
    fn test_module_properties_with_services() {
        let m = TestModule;
        let props = m.properties();
        assert_eq!(props.required_services.len(), 1);
        assert_eq!(props.required_services[0].kind, ServiceKind::Postgres);
        assert_eq!(props.required_modules, vec!["order"]);
    }

    #[test]
    fn test_service_call_callback() {
        let mut ctx = ModuleContext::new();
        ctx.call_service = Some(Arc::new(|kind: &str, id: &str, payload: &[u8]| {
            assert_eq!(kind, "postgres");
            assert_eq!(id, "main");
            assert_eq!(payload, b"SELECT 1");
            b"ok".to_vec()
        }));

        let result = ctx.call_service.as_ref().unwrap()("postgres", "main", b"SELECT 1");
        assert_eq!(result, b"ok");
    }

    #[test]
    fn test_module_call_callback() {
        let mut ctx = ModuleContext::new();
        ctx.call_module = Some(Arc::new(|module: &str, func: &str, args: &[u8]| {
            assert_eq!(module, "order");
            assert_eq!(func, "calc");
            assert_eq!(args, b"{}");
            b"42".to_vec()
        }));

        let result = ctx.call_module.as_ref().unwrap()("order", "calc", b"{}");
        assert_eq!(result, b"42");
    }

    // -- FromModuleBytes tests -----------------------------------------------

    #[test]
    fn test_from_bytes_vec_u8() {
        let v: Vec<u8> = FromModuleBytes::from_module_bytes(b"hello").unwrap();
        assert_eq!(v, b"hello");
    }

    #[test]
    fn test_from_bytes_string() {
        let s: String = FromModuleBytes::from_module_bytes(b"hello world").unwrap();
        assert_eq!(s, "hello world");
    }

    #[test]
    fn test_from_bytes_i32() {
        let n: i32 = FromModuleBytes::from_module_bytes(b"42").unwrap();
        assert_eq!(n, 42);
    }

    #[test]
    fn test_from_bytes_f64() {
        let n: f64 = FromModuleBytes::from_module_bytes(b"3.14").unwrap();
        assert!((n - 3.14).abs() < 0.001);
    }

    #[test]
    fn test_from_bytes_bool() {
        let b: bool = FromModuleBytes::from_module_bytes(b"true").unwrap();
        assert!(b);
        let b: bool = FromModuleBytes::from_module_bytes(b"false").unwrap();
        assert!(!b);
    }

    #[test]
    fn test_from_bytes_invalid_number() {
        let result: Result<i32, _> = FromModuleBytes::from_module_bytes(b"not a number");
        assert!(result.is_err());
    }

    // -- Typed call tests -----------------------------------------------------

    #[test]
    fn test_call_module_typed_string() {
        let mut ctx = ModuleContext::new();
        ctx.call_module = Some(Arc::new(|_: &str, _: &str, _: &[u8]| {
            b"Alice".to_vec()
        }));

        let name: String = ctx.call_module_typed("user", "get_name", b"{}").unwrap();
        assert_eq!(name, "Alice");
    }

    #[test]
    fn test_call_module_typed_i32() {
        let mut ctx = ModuleContext::new();
        ctx.call_module = Some(Arc::new(|_: &str, _: &str, _: &[u8]| {
            b"99".to_vec()
        }));

        let count: i32 = ctx.call_module_typed("order", "count", b"{}").unwrap();
        assert_eq!(count, 99);
    }

    #[test]
    fn test_call_module_typed_no_callback() {
        let ctx = ModuleContext::new();
        let result: Result<String, _> = ctx.call_module_typed("user", "f", b"{}");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not set"));
    }

    #[test]
    fn test_call_service_typed() {
        let mut ctx = ModuleContext::new();
        ctx.call_service = Some(Arc::new(|_: &str, _: &str, _: &[u8]| {
            b"200".to_vec()
        }));

        let count: i32 = ctx.call_service_typed("redis", "cache", b"GET counter").unwrap();
        assert_eq!(count, 200);
    }
}
