//! Integration test WASM module — zero allocations, fixed buffers.

unsafe extern "C" {
    fn register_route(m_ptr: *const u8, m_len: i32, p_ptr: *const u8, p_len: i32);
    fn call_service(payload_ptr: *const u8, payload_len: i32) -> i32;
}

fn write_at(s: &str, offset: usize) -> (*const u8, usize) {
    let bytes = s.as_bytes();
    unsafe {
        for (i, &b) in bytes.iter().enumerate() {
            *(offset as *mut u8).add(i) = b;
        }
    }
    (offset as *const u8, bytes.len())
}

/// Build service-call JSON directly into a stack buffer and call the host.
fn call_service_json(kind: &str, id: &str, payload: &str) -> i32 {
    let mut buf = [0u8; 1024];
    let mut pos = 0usize;
    let write = |buf: &mut [u8; 1024], pos: &mut usize, s: &[u8]| {
        for b in s { buf[*pos] = *b; *pos += 1; }
    };
    write(&mut buf, &mut pos, b"{\"kind\":\"");
    write(&mut buf, &mut pos, kind.as_bytes());
    write(&mut buf, &mut pos, b"\",\"id\":\"");
    write(&mut buf, &mut pos, id.as_bytes());
    write(&mut buf, &mut pos, b"\",\"payload\":\"");
    write(&mut buf, &mut pos, payload.as_bytes());
    write(&mut buf, &mut pos, b"\"}");
    unsafe { call_service(buf.as_ptr(), pos as i32) }
}

#[unsafe(no_mangle)]
pub extern "C" fn init() {
    let (m, ml) = write_at("GET", 100);
    let (p, pl) = write_at("/wasm-test", 200);
    unsafe { register_route(m, ml as i32, p, pl as i32); }

    // Call all 4 services — result goes to WASM memory at offset 0
    call_service_json("postgres", "main_db", "SELECT 1");
    call_service_json("redis", "cache", "{\"cmd\":\"GET\",\"key\":\"test\"}");
    call_service_json("s3", "assets", "{\"cmd\":\"GET\",\"bucket\":\"t\",\"key\":\"h\"}");
    call_service_json("http", "default", "{\"method\":\"GET\",\"url\":\"http://x\"}");
}
