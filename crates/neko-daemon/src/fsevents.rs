//! A small binding of macOS's FSEvents (CoreServices), for files watches (docs/knowledge/waits.md). One stream per watch, on the
//! watch's folder, delivered on one serial dispatch queue: the callback only notes which paths changed and calls `on_change`; the
//! scheduler thread decides what to do with them. Written here rather than taken from a crate: the usual one is CC0-licensed, and
//! Neko's dependencies are MIT or Apache-2.0 only; this is the few calls Neko needs.

use std::ffi::{CStr, c_char, c_void};
use std::sync::OnceLock;

type CFAllocatorRef = *const c_void;
type CFArrayRef = *const c_void;
type CFStringRef = *const c_void;
type FSEventStreamRef = *mut c_void;
type DispatchQueue = *mut c_void;

#[repr(C)]
struct StreamContext {
    version: isize,
    info: *mut c_void,
    retain: Option<extern "C" fn(*const c_void) -> *const c_void>,
    release: Option<extern "C" fn(*const c_void)>,
    copy_description: Option<extern "C" fn(*const c_void) -> CFStringRef>,
}

type Callback = extern "C" fn(FSEventStreamRef, *mut c_void, usize, *mut c_void, *const u32, *const u64);

#[link(name = "CoreServices", kind = "framework")]
unsafe extern "C" {
    fn FSEventStreamCreate(allocator: CFAllocatorRef, callback: Callback, context: *const StreamContext, paths: CFArrayRef, since: u64, latency: f64, flags: u32) -> FSEventStreamRef;
    fn FSEventStreamSetDispatchQueue(stream: FSEventStreamRef, queue: DispatchQueue);
    fn FSEventStreamStart(stream: FSEventStreamRef) -> u8;
    fn FSEventStreamStop(stream: FSEventStreamRef);
    fn FSEventStreamInvalidate(stream: FSEventStreamRef);
    fn FSEventStreamRelease(stream: FSEventStreamRef);
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFTypeArrayCallBacks: c_void;
    fn CFStringCreateWithBytes(alloc: CFAllocatorRef, bytes: *const u8, len: isize, encoding: u32, external: u8) -> CFStringRef;
    fn CFArrayCreate(alloc: CFAllocatorRef, values: *const *const c_void, n: isize, callbacks: *const c_void) -> CFArrayRef;
    fn CFRelease(cf: *const c_void);
}

unsafe extern "C" {
    fn dispatch_queue_create(label: *const c_char, attr: *const c_void) -> DispatchQueue;
}

const UTF8: u32 = 0x0800_0100;
const SINCE_NOW: u64 = u64::MAX;
/// No defer (the first change in a quiet spell is told at once), file-level events, and watch the root itself (its removal too).
const FLAGS: u32 = 0x02 | 0x04 | 0x10;
/// Changes within this long of each other come in one call (seconds).
const LATENCY: f64 = 1.0;

/// What a stream calls with the paths that changed (absolute, as FSEvents gives them: symlinks in the folder's own path resolved).
pub type OnChange = Box<dyn Fn(Vec<String>) + Send + Sync>;

/// One serial queue for every stream: their callbacks never run at once.
struct Queue(DispatchQueue);
unsafe impl Send for Queue {}
unsafe impl Sync for Queue {}

fn queue() -> DispatchQueue {
    static Q: OnceLock<Queue> = OnceLock::new();
    Q.get_or_init(|| Queue(unsafe { dispatch_queue_create(c"neko.watches".as_ptr(), std::ptr::null()) })).0
}

extern "C" fn release(info: *const c_void) {
    // The stream is done with its callback: the boxed closure goes.
    drop(unsafe { Box::from_raw(info as *mut OnChange) });
}

extern "C" fn changed(_stream: FSEventStreamRef, info: *mut c_void, n: usize, paths: *mut c_void, _flags: *const u32, _ids: *const u64) {
    if info.is_null() || paths.is_null() {
        return;
    }
    let f = unsafe { &*(info as *const OnChange) };
    let list = paths as *const *const c_char;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let p = unsafe { *list.add(i) };
        if !p.is_null() {
            out.push(unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned());
        }
    }
    if !out.is_empty() {
        f(out);
    }
}

/// A running stream; dropping it stops it.
pub struct Stream(FSEventStreamRef);
unsafe impl Send for Stream {}

impl Drop for Stream {
    fn drop(&mut self) {
        unsafe {
            FSEventStreamStop(self.0);
            FSEventStreamInvalidate(self.0);
            FSEventStreamRelease(self.0);
        }
    }
}

/// Watches `folder` and everything under it; `on_change` hears the paths that change. None when FSEvents would not start.
pub fn watch(folder: &str, on_change: OnChange) -> Option<Stream> {
    let info = Box::into_raw(Box::new(on_change)) as *mut c_void;
    let ctx = StreamContext { version: 0, info, retain: None, release: Some(release), copy_description: None };
    unsafe {
        let s = CFStringCreateWithBytes(std::ptr::null(), folder.as_ptr(), folder.len() as isize, UTF8, 0);
        if s.is_null() {
            drop(Box::from_raw(info as *mut OnChange));
            return None;
        }
        let values = [s];
        let arr = CFArrayCreate(std::ptr::null(), values.as_ptr(), 1, &raw const kCFTypeArrayCallBacks);
        CFRelease(s);
        if arr.is_null() {
            drop(Box::from_raw(info as *mut OnChange));
            return None;
        }
        let stream = FSEventStreamCreate(std::ptr::null(), changed, &ctx, arr, SINCE_NOW, LATENCY, FLAGS);
        CFRelease(arr);
        if stream.is_null() {
            // The context's release was not taken over by a stream.
            drop(Box::from_raw(info as *mut OnChange));
            return None;
        }
        FSEventStreamSetDispatchQueue(stream, queue());
        if FSEventStreamStart(stream) == 0 {
            FSEventStreamInvalidate(stream);
            FSEventStreamRelease(stream);
            return None;
        }
        Some(Stream(stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    #[test]
    fn a_change_in_the_folder_is_heard_and_a_stopped_stream_hears_nothing() {
        let dir = std::env::temp_dir().join(format!("neko-fsevents-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let root = std::fs::canonicalize(&dir).unwrap().to_string_lossy().into_owned();
        let heard: Arc<Mutex<Vec<String>>> = Arc::default();
        let h = heard.clone();
        let s = watch(&root, Box::new(move |p| h.lock().unwrap().extend(p))).expect("FSEvents starts");
        std::thread::sleep(Duration::from_millis(300));
        std::fs::write(dir.join("build.log"), "done").unwrap();
        let until = Instant::now() + Duration::from_secs(10);
        while Instant::now() < until && !heard.lock().unwrap().iter().any(|p| p.ends_with("/build.log")) {
            std::thread::sleep(Duration::from_millis(100));
        }
        assert!(heard.lock().unwrap().iter().any(|p| p == &format!("{root}/build.log")), "heard {:?}", heard.lock().unwrap());
        drop(s);
        heard.lock().unwrap().clear();
        std::fs::write(dir.join("later.log"), "x").unwrap();
        std::thread::sleep(Duration::from_millis(2_000));
        assert!(heard.lock().unwrap().is_empty(), "a stopped stream hears nothing");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
