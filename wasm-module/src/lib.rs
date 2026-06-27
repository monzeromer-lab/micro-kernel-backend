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

impl Method {
    pub fn as_str(&self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Delete => "DELETE",
            Method::Patch => "PATCH",
        }
    }
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

    // -- Typed handle tests ---------------------------------------------------

    struct MockPostgres;
    impl PostgresHandle for MockPostgres {
        fn query(&self, sql: &str) -> Result<String, String> {
            Ok(format!("result: {sql}"))
        }
        fn execute(&self, _sql: &str) -> Result<u64, String> { Ok(42) }
        fn query_with(&self, sql: &str, _params: &[&str]) -> Result<String, String> {
            Ok(format!("param: {sql}"))
        }
    }
    impl MySqlHandle for MockPostgres {
        fn query(&self, sql: &str) -> Result<String, String> { Ok(format!("mysql: {sql}")) }
        fn execute(&self, _sql: &str) -> Result<u64, String> { Ok(99) }
        fn query_with(&self, sql: &str, _params: &[&str]) -> Result<String, String> { Ok(format!("mysql_param: {sql}")) }
    }

    #[test]
    fn test_typed_postgres_handle() {
        let mut ctx = ModuleContext::new();
        ctx.postgres = Some(Arc::new(MockPostgres));

        let pg = ctx.postgres.clone().unwrap();
        assert_eq!(pg.query("SELECT 1").unwrap(), "result: SELECT 1");
        assert_eq!(pg.execute("DELETE").unwrap(), 42);
        assert_eq!(pg.query_with("SELECT $1", &["x"]).unwrap(), "param: SELECT $1");
    }

    struct MockRedis;
    impl RedisHandle for MockRedis {
        fn get(&self, key: &str) -> Result<Option<String>, String> {
            Ok(Some(format!("val:{key}")))
        }
        fn set(&self, _k: &str, _v: &str, _t: Option<u64>) -> Result<(), String> { Ok(()) }
        fn del(&self, keys: &[&str]) -> Result<u64, String> { Ok(keys.len() as u64) }
        fn incr(&self, _k: &str, _a: Option<i64>) -> Result<i64, String> { Ok(100) }
        fn exists(&self, _k: &str) -> Result<bool, String> { Ok(true) }
    }

    #[test]
    fn test_typed_redis_handle() {
        let mut ctx = ModuleContext::new();
        ctx.redis = Some(Arc::new(MockRedis));

        let r = ctx.redis.clone().unwrap();
        assert_eq!(r.get("k").unwrap(), Some("val:k".into()));
        assert!(r.set("k", "v", None).is_ok());
        assert_eq!(r.del(&["a", "b"]).unwrap(), 2);
        assert_eq!(r.incr("c", None).unwrap(), 100);
        assert!(r.exists("d").unwrap());
    }

    struct MockS3;
    impl S3Handle for MockS3 {
        fn put(&self, bucket: &str, key: &str, _data: &[u8]) -> Result<String, String> {
            Ok(format!("{bucket}/{key}"))
        }
        fn get(&self, bucket: &str, key: &str) -> Result<Vec<u8>, String> {
            Ok(format!("{bucket}/{key}").into_bytes())
        }
        fn delete(&self, _b: &str, _k: &str) -> Result<bool, String> { Ok(true) }
        fn list(&self, _b: &str, _p: Option<&str>) -> Result<String, String> {
            Ok("<ListBucketResult/>".into())
        }
    }

    #[test]
    fn test_typed_s3_handle() {
        let mut ctx = ModuleContext::new();
        ctx.s3 = Some(Arc::new(MockS3));

        let s = ctx.s3.clone().unwrap();
        assert_eq!(s.put("b", "k.txt", b"data").unwrap(), "b/k.txt");
        assert_eq!(s.get("b", "k.txt").unwrap(), b"b/k.txt");
        assert!(s.delete("b", "k.txt").unwrap());
        assert_eq!(s.list("b", None).unwrap(), "<ListBucketResult/>");
    }

    struct MockHttp;
    impl HttpHandle for MockHttp {
        fn get(&self, url: &str) -> Result<String, String> { Ok(format!("GET:{url}")) }
        fn post(&self, url: &str, body: &str) -> Result<String, String> { Ok(format!("POST:{url}:{body}")) }
        fn put(&self, url: &str, body: &str) -> Result<String, String> { Ok(format!("PUT:{url}:{body}")) }
        fn delete(&self, url: &str) -> Result<String, String> { Ok(format!("DELETE:{url}")) }
    }

    #[test]
    fn test_typed_http_handle() {
        let mut ctx = ModuleContext::new();
        ctx.http = Some(Arc::new(MockHttp));

        let h = ctx.http.clone().unwrap();
        assert_eq!(h.get("http://a").unwrap(), "GET:http://a");
        assert_eq!(h.post("http://a", "b").unwrap(), "POST:http://a:b");
        assert_eq!(h.put("http://a", "b").unwrap(), "PUT:http://a:b");
        assert_eq!(h.delete("http://a").unwrap(), "DELETE:http://a");
    }

    // -- Nested scope handle propagation tests ---------------------------------

    #[test]
    fn test_nested_scope_handle_propagation() {
        let mut ctx = ModuleContext::new();
        ctx.postgres = Some(Arc::new(MockPostgres));
        ctx.redis = Some(Arc::new(MockRedis));

        ctx.scope("/api", |sub| {
            // Nested scope should have inherited the handles
            assert!(sub.postgres.is_some());
            assert!(sub.redis.is_some());
            let pg = sub.postgres.clone().unwrap();
            assert_eq!(pg.query("SELECT 1").unwrap(), "result: SELECT 1");
        });

        assert_eq!(ctx.scopes.len(), 1);
    }

    // -- Multiple exports test -------------------------------------------------

    #[test]
    fn test_multiple_exports() {
        let mut ctx = ModuleContext::new();
        ctx.export("fn_a").export("fn_b").export("fn_c");

        let exports: Vec<&String> = ctx.exports().collect();
        assert_eq!(exports, vec!["fn_a", "fn_b", "fn_c"]);
    }

    // -- Edge case tests -------------------------------------------------------

    #[test]
    fn test_empty_context() {
        let ctx = ModuleContext::new();
        assert_eq!(ctx.routes().count(), 0);
        assert_eq!(ctx.scopes().count(), 0);
        assert_eq!(ctx.exports().count(), 0);
        assert_eq!(ctx.middleware_entries().count(), 0);
        assert_eq!(ctx.guard_entries().count(), 0);
        assert!(ctx.call_service.is_none());
        assert!(ctx.call_module.is_none());
        assert!(ctx.postgres.is_none());
        assert!(ctx.redis.is_none());
    }

    #[test]
    fn test_multiple_routes_same_path_different_methods() {
        let mut ctx = ModuleContext::new();
        ctx.get("/path", || "get")
           .post("/path", || "post")
           .put("/path", || "put");

        let routes: Vec<&RouteDef> = ctx.routes().collect();
        assert_eq!(routes.len(), 3);
        assert_eq!(routes[0].method, Method::Get);
        assert_eq!(routes[1].method, Method::Post);
        assert_eq!(routes[2].method, Method::Put);
        // All same path
        assert_eq!(routes[0].path, "/path");
        assert_eq!(routes[1].path, "/path");
    }

    #[test]
    fn test_deeply_nested_scopes() {
        let mut ctx = ModuleContext::new();
        ctx.scope("/a", |a| {
            a.scope("/b", |b| {
                b.scope("/c", |c| {
                    c.get("/leaf", || "deep");
                });
            });
        });

        assert_eq!(ctx.scopes().count(), 1);
        let a = &ctx.scopes().collect::<Vec<_>>()[0];
        assert_eq!(a.prefix, "/a");
        let b = &a.context.scopes().collect::<Vec<_>>()[0];
        assert_eq!(b.prefix, "/b");
        let c = &b.context.scopes().collect::<Vec<_>>()[0];
        assert_eq!(c.prefix, "/c");
        assert_eq!(c.context.routes().count(), 1);
    }

    #[test]
    fn test_middleware_name() {
        struct LoggerMw;
        impl Middleware for LoggerMw {
            fn name(&self) -> Cow<'static, str> { "logger".into() }
            fn before(&self) -> bool { false }
            fn after(&self) -> bool { false }
        }

        let mw = LoggerMw;
        assert_eq!(mw.name(), "logger");
        assert!(!mw.before());
        assert!(!mw.after());
    }

    #[test]
    fn test_guard_name_and_check() {
        struct RateLimitGuard;
        impl Guard for RateLimitGuard {
            fn name(&self) -> Cow<'static, str> { "ratelimit".into() }
            fn check(&self) -> bool { true }
        }

        let guard = RateLimitGuard;
        assert_eq!(guard.name(), "ratelimit");
        assert!(guard.check());
    }

    #[test]
    fn test_handler_closure_with_state() {
        let captured = "hello".to_string();
        let handler = move || captured.clone();
        let resp = handler.call();
        assert_eq!(resp.status, 200);
        assert_eq!(String::from_utf8(resp.body).unwrap(), "hello");
    }

    #[test]
    fn test_response_custom_headers() {
        let resp = Response {
            status: 201,
            headers: vec![("x-custom".into(), "val".into())],
            body: b"body".to_vec(),
        };
        assert_eq!(resp.status, 201);
        assert_eq!(resp.headers[0].0, "x-custom");
        assert_eq!(resp.body, b"body");
    }

    #[test]
    fn test_from_bytes_empty() {
        let v: Vec<u8> = FromModuleBytes::from_module_bytes(b"").unwrap();
        assert!(v.is_empty());
        let s: String = FromModuleBytes::from_module_bytes(b"").unwrap();
        assert!(s.is_empty());
    }

    #[test]
    fn test_from_bytes_invalid_utf8_for_string() {
        let result: Result<String, _> = FromModuleBytes::from_module_bytes(&[0xFF, 0xFE]);
        assert!(result.is_err());
    }

    #[test]
    fn test_scope_propagates_all_handles() {
        let mut ctx = ModuleContext::new();
        ctx.postgres = Some(Arc::new(MockPostgres));
        ctx.redis = Some(Arc::new(MockRedis));
        ctx.mysql = Some(Arc::new(MockPostgres)); // same mock works
        ctx.http = Some(Arc::new(MockHttp));
        ctx.s3 = Some(Arc::new(MockS3));

        ctx.scope("/sub", |sub| {
            assert!(sub.postgres.is_some());
            assert!(sub.redis.is_some());
            assert!(sub.mysql.is_some());
            assert!(sub.http.is_some());
            assert!(sub.s3.is_some());
        });
    }

    #[test]
    fn test_method_as_str() {
        assert_eq!(Method::Get.as_str(), "GET");
        assert_eq!(Method::Post.as_str(), "POST");
        assert_eq!(Method::Put.as_str(), "PUT");
        assert_eq!(Method::Delete.as_str(), "DELETE");
        assert_eq!(Method::Patch.as_str(), "PATCH");
    }

    #[test]
    fn test_module_properties_full() {
        let props = ModuleProperties {
            memory_pages: 10,
            max_memory_pages: Some(100),
            memory64: true,
            consume_fuel: true,
            max_wasm_stack: Some(1_048_576),
            required_services: vec![
                ServiceRequirement { kind: ServiceKind::Postgres, identifier: "db".into() },
                ServiceRequirement { kind: ServiceKind::S3, identifier: "files".into() },
            ],
            required_modules: vec!["auth".into(), "logging".into()],
        };

        assert_eq!(props.memory_pages, 10);
        assert!(props.memory64);
        assert!(props.consume_fuel);
        assert_eq!(props.required_services.len(), 2);
        assert_eq!(props.required_modules.len(), 2);
    }
}
