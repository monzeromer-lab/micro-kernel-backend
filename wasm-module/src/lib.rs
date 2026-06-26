use std::borrow::Cow;

// ---------------------------------------------------------------------------
// WasmModule — the contract every module must implement
// ---------------------------------------------------------------------------

pub trait WasmModule {
    /// Called once when the module is loaded. Register routes, middleware,
    /// guards, and nested scopes using the provided [`ModuleContext`].
    fn register(&self, ctx: &mut ModuleContext);

    /// Declare the runtime properties this module needs.
    fn properties() -> ModuleProperties {
        ModuleProperties::default()
    }

    /// Semantic version of this module — used for blue-green deployments.
    fn version() -> (u16, u16, u16) {
        (0, 1, 0)
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
}

impl Default for ModuleProperties {
    fn default() -> Self {
        Self {
            memory_pages: 1,
            max_memory_pages: None,
            memory64: false,
            consume_fuel: false,
            max_wasm_stack: None,
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
// Middleware — must be a proper trait for the talk
// ---------------------------------------------------------------------------

/// A request/response interceptor.
///
/// Implement this trait to create middleware that runs before or after
/// every route in a scope. Modules register middleware via
/// [`ModuleContext::middleware`].
pub trait Middleware: Send + Sync + 'static {
    /// Unique name for this middleware (used in logs / dashboard).
    fn name(&self) -> Cow<'static, str>;

    /// Called **before** the handler runs. Return `true` to continue to the
    /// handler, or `false` to short-circuit (the request is rejected).
    fn before(&self) -> bool {
        true
    }

    /// Called **after** the handler runs. The response can be inspected or
    /// mutated here (future: the actual response object will be passed in).
    fn after(&self) -> bool {
        true
    }
}

// ---------------------------------------------------------------------------
// Guard — must be a proper trait for the talk
// ---------------------------------------------------------------------------

/// A conditional gate that must pass before a route executes.
///
/// Implement this trait to create guards for auth, header validation, etc.
/// Modules register guards via [`ModuleContext::guard`].
pub trait Guard: Send + Sync + 'static {
    /// Unique name for this guard (used in logs / dashboard).
    fn name(&self) -> Cow<'static, str>;

    /// Return `true` if the request is allowed through. Return `false` to
    /// reject with 403 Forbidden.
    fn check(&self) -> bool;
}

// ---------------------------------------------------------------------------
// ModuleContext
// ---------------------------------------------------------------------------

pub struct ModuleContext {
    routes: Vec<RouteDef>,
    scopes: Vec<ScopeDef>,
    middleware: Vec<Box<dyn Middleware>>,
    guards: Vec<Box<dyn Guard>>,
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
        f(&mut sub);
        self.scopes.push(ScopeDef {
            prefix: prefix.into(),
            context: sub,
        });
        self
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
            ctx.get("/", || "hello");
            ctx.scope("/admin", |admin| {
                admin.get("/dashboard", || "admin dashboard");
            });
        }

        fn properties() -> ModuleProperties {
            ModuleProperties {
                memory_pages: 2,
                ..Default::default()
            }
        }

        fn version() -> (u16, u16, u16) {
            (1, 2, 3)
        }
    }

    #[test]
    fn test_module_registers_routes() {
        let mut ctx = ModuleContext::new();
        TestModule.register(&mut ctx);

        assert_eq!(ctx.routes.len(), 1);
        assert_eq!(ctx.scopes.len(), 1);
        assert_eq!(ctx.scopes[0].prefix, "/admin");
    }

    #[test]
    fn test_module_builder_pattern() {
        let mut ctx = ModuleContext::new();
        ctx.get("/a", || "a")
           .post("/b", || "b")
           .put("/c", || "c")
           .delete("/d", || "d")
           .patch("/e", || "e");

        assert_eq!(ctx.routes.len(), 5);
    }

    #[test]
    fn test_response_methods() {
        assert_eq!(Response::ok("hi").status, 200);
        assert_eq!(Response::created("new").status, 201);
        assert_eq!(Response::bad_request("no").status, 400);
        assert_eq!(Response::not_found().status, 404);
        assert_eq!(Response::internal_error("oops").status, 500);
    }

    #[test]
    fn test_middleware_trait() {
        struct AuthMw;
        impl Middleware for AuthMw {
            fn name(&self) -> Cow<'static, str> { "auth".into() }
            fn before(&self) -> bool { true }
            fn after(&self) -> bool { true }
        }

        let mut ctx = ModuleContext::new();
        ctx.middleware(AuthMw);
        assert_eq!(ctx.middleware_entries().count(), 1);
    }

    #[test]
    fn test_guard_trait() {
        struct AdminGuard;
        impl Guard for AdminGuard {
            fn name(&self) -> Cow<'static, str> { "admin".into() }
            fn check(&self) -> bool { false }
        }

        let mut ctx = ModuleContext::new();
        ctx.guard(AdminGuard);
        assert_eq!(ctx.guard_entries().count(), 1);
    }

    #[test]
    fn test_module_version() {
        assert_eq!(TestModule::version(), (1, 2, 3));
    }
}
