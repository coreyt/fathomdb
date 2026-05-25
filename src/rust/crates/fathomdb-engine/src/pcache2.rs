// 0.7.0 perf-experiments: prototype `SQLITE_CONFIG_PCACHE2` custom
// page-cache implementation.
//
// Hypothesis: AC-020's residual concurrent-mutex contention after
// the W4.0j canonical-SQLite stack (MEMSTATUS=0 + PAGECACHE
// pre-alloc + page_size=8K + reader cache/mmap/temp/sync) is on
// SQLite's default `pcache1` per-instance mutex, which is shared
// across all reader connections opening the same database file via
// the shared-cache allocator. Replacing the implementation with a
// per-instance Rust `Mutex` (not a global mutex; no cross-instance
// contention) should close the remaining 14% gap to the 5.33×
// AC-020 contract.
//
// Architecture:
// - One `Cache` instance per SQLite pcache (xCreate returns one
//   per database/connection).
// - Pages keyed by SQLite's u32 page number.
// - Per-instance `parking_lot::Mutex<CacheState>` guards the
//   hashmap + LRU list.
// - Page memory is laid out as `[ pBuf: szPage bytes ] [ pExtra: szExtra bytes ]`
//   in a single Box<[u8]>. SQLite expects pBuf and pExtra as
//   separate pointers in the returned sqlite3_pcache_page struct.
//
// Activation: process-start `sqlite3_config(SQLITE_CONFIG_PCACHE2,
// &methods)` via the existing `init_perf_experiments_runtime()`
// Once block when FATHOMDB_PERF_SQLITE_PCACHE2=1.
//
// **Prototype scope.** Correctness over performance. Not yet
// thread-affined; not yet using lock-free hashmap. Validates the
// hypothesis only — if it KEEPS, a production-grade implementation
// follows.
//
// References:
// - sqlite3.h `sqlite3_pcache_methods2` documentation.
// - `dev/notes/performance-whitepaper-notes.md` §6 (H1 hypothesis).
// - `dev/adr/ADR-0.7.0-ac020-architectural-lever.md` Option 1.
// - `dev/plans/0.7.0-perf-experiments.md` Wave 4 entries W4.0d..j
//   for the canonical-SQLite-stack baseline this prototype is
//   evaluated against.

use std::collections::HashMap;
use std::os::raw::{c_int, c_uint, c_void};
use std::sync::Mutex;

use rusqlite::ffi;

// Each cache instance owns a Mutex<State> guarding its hashmap.
struct Cache {
    sz_page: c_int,
    sz_extra: c_int,
    purgeable: bool,
    state: Mutex<State>,
}

struct State {
    pages: HashMap<c_uint, Box<Page>>,
    /// LRU ordering (front = LRU, back = MRU). Entries are page keys
    /// that are currently unpinned.
    lru: Vec<c_uint>,
    /// Soft limit on the number of pages SQLite asked us to retain.
    /// `0` means unlimited.
    cache_size_hint: c_int,
}

struct Page {
    /// Combined backing buffer: `sz_page` bytes for pBuf, then
    /// `sz_extra` bytes for pExtra. Heap-allocated and pinned
    /// because SQLite stores raw pointers into it.
    buffer: Vec<u8>,
    sz_page: c_int,
    pinned: bool,
}

impl Page {
    fn buf_ptr(&mut self) -> *mut c_void {
        self.buffer.as_mut_ptr().cast::<c_void>()
    }
    fn extra_ptr(&mut self) -> *mut c_void {
        // SAFETY: buffer length is sz_page + sz_extra; offset by
        // sz_page is in-bounds.
        unsafe { self.buffer.as_mut_ptr().add(self.sz_page as usize).cast::<c_void>() }
    }
}

// Note: SQLite's sqlite3_pcache_page layout is `{ pBuf, pExtra }` —
// we use rusqlite::ffi::sqlite3_pcache_page directly (see page_repr
// below). No local mirror struct is needed; the historical
// `OurPcachePage` was removed.

// SQLite treats sqlite3_pcache_page as a header — the layout MUST
// match. rusqlite::ffi::sqlite3_pcache_page has the same shape:
// { pBuf: *mut c_void, pExtra: *mut c_void }. We allocate one
// inline per Page so the pointer stays stable until the page is
// freed.
fn page_repr(p: &mut Page) -> ffi::sqlite3_pcache_page {
    ffi::sqlite3_pcache_page { pBuf: p.buf_ptr(), pExtra: p.extra_ptr() }
}

// -- sqlite3_pcache_methods2 callbacks --

unsafe extern "C" fn pcache_init(_arg: *mut c_void) -> c_int {
    ffi::SQLITE_OK
}

unsafe extern "C" fn pcache_shutdown(_arg: *mut c_void) {}

unsafe extern "C" fn pcache_create(
    sz_page: c_int,
    sz_extra: c_int,
    purgeable: c_int,
) -> *mut ffi::sqlite3_pcache {
    let cache = Box::new(Cache {
        sz_page,
        sz_extra,
        purgeable: purgeable != 0,
        state: Mutex::new(State { pages: HashMap::new(), lru: Vec::new(), cache_size_hint: 0 }),
    });
    Box::into_raw(cache).cast::<ffi::sqlite3_pcache>()
}

unsafe extern "C" fn pcache_cachesize(cache: *mut ffi::sqlite3_pcache, n_cachesize: c_int) {
    let cache = &*cache.cast::<Cache>();
    let mut s = cache.state.lock().unwrap();
    s.cache_size_hint = n_cachesize;
}

unsafe extern "C" fn pcache_pagecount(cache: *mut ffi::sqlite3_pcache) -> c_int {
    let cache = &*cache.cast::<Cache>();
    let s = cache.state.lock().unwrap();
    c_int::try_from(s.pages.len()).unwrap_or(c_int::MAX)
}

unsafe extern "C" fn pcache_fetch(
    cache: *mut ffi::sqlite3_pcache,
    key: c_uint,
    create_flag: c_int,
) -> *mut ffi::sqlite3_pcache_page {
    let cache_ref: &Cache = &*cache.cast::<Cache>();
    let mut s = cache_ref.state.lock().unwrap();

    if let Some(page) = s.pages.get_mut(&key) {
        page.pinned = true;
        // Drop from LRU since it's now pinned.
        s.lru.retain(|k| *k != key);
        let page = s.pages.get_mut(&key).unwrap();
        // Allocate a per-call sqlite3_pcache_page struct on the heap;
        // SQLite hands this back to xUnpin/xRekey so it must outlive
        // the xFetch call. We embed it in the Page itself via the
        // page's leading-zero offset: since we just need a stable
        // pointer, allocate a tiny boxed struct here.
        let pp = Box::new(page_repr(page));
        return Box::into_raw(pp);
    }

    if create_flag == 0 {
        return std::ptr::null_mut();
    }

    // Evict if at capacity (best-effort).
    if cache_ref.purgeable && s.cache_size_hint > 0 {
        let limit = s.cache_size_hint as usize;
        while s.pages.len() >= limit {
            let evict = match s.lru.first().copied() {
                Some(k) => k,
                None => break, // no unpinned page available
            };
            s.lru.remove(0);
            s.pages.remove(&evict);
        }
    }

    let total = (cache_ref.sz_page + cache_ref.sz_extra).max(1) as usize;
    let mut page =
        Box::new(Page { buffer: vec![0_u8; total], sz_page: cache_ref.sz_page, pinned: true });
    let pp = Box::new(page_repr(&mut page));
    s.pages.insert(key, page);
    Box::into_raw(pp)
}

unsafe extern "C" fn pcache_unpin(
    cache: *mut ffi::sqlite3_pcache,
    page_ptr: *mut ffi::sqlite3_pcache_page,
    discard: c_int,
) {
    if page_ptr.is_null() {
        return;
    }
    let cache_ref: &Cache = &*cache.cast::<Cache>();
    // Find the page by matching its pBuf address.
    let buf_addr = (*page_ptr).pBuf as usize;
    // Drop the heap-allocated pcache_page struct.
    drop(Box::from_raw(page_ptr));

    let mut s = cache_ref.state.lock().unwrap();
    // Find key whose page's first byte == buf_addr.
    let key = s
        .pages
        .iter_mut()
        .find_map(|(k, p)| (p.buffer.as_ptr() as usize == buf_addr).then_some(*k));
    let Some(key) = key else {
        return;
    };
    if discard != 0 {
        s.pages.remove(&key);
        s.lru.retain(|k| *k != key);
    } else if let Some(p) = s.pages.get_mut(&key) {
        p.pinned = false;
        if !s.lru.contains(&key) {
            s.lru.push(key);
        }
    }
}

unsafe extern "C" fn pcache_rekey(
    cache: *mut ffi::sqlite3_pcache,
    _page_ptr: *mut ffi::sqlite3_pcache_page,
    old_key: c_uint,
    new_key: c_uint,
) {
    let cache_ref: &Cache = &*cache.cast::<Cache>();
    let mut s = cache_ref.state.lock().unwrap();
    if let Some(page) = s.pages.remove(&old_key) {
        s.pages.insert(new_key, page);
    }
    for k in s.lru.iter_mut() {
        if *k == old_key {
            *k = new_key;
        }
    }
}

unsafe extern "C" fn pcache_truncate(cache: *mut ffi::sqlite3_pcache, i_limit: c_uint) {
    let cache_ref: &Cache = &*cache.cast::<Cache>();
    let mut s = cache_ref.state.lock().unwrap();
    let drop_keys: Vec<c_uint> = s.pages.keys().copied().filter(|k| *k >= i_limit).collect();
    for k in drop_keys {
        s.pages.remove(&k);
        s.lru.retain(|x| *x != k);
    }
}

unsafe extern "C" fn pcache_destroy(cache: *mut ffi::sqlite3_pcache) {
    if cache.is_null() {
        return;
    }
    drop(Box::from_raw(cache.cast::<Cache>()));
}

unsafe extern "C" fn pcache_shrink(cache: *mut ffi::sqlite3_pcache) {
    let cache_ref: &Cache = &*cache.cast::<Cache>();
    let mut s = cache_ref.state.lock().unwrap();
    // Free all unpinned pages.
    let to_evict: Vec<c_uint> = s.lru.drain(..).collect();
    for k in to_evict {
        s.pages.remove(&k);
    }
}

/// The methods table, installed once via sqlite3_config(SQLITE_CONFIG_PCACHE2).
/// Wrapped in a Sync-asserting newtype because the struct contains a
/// `*mut c_void` pArg field (always null here, never mutated) which
/// otherwise blocks `Sync` derivation on `static`.
#[repr(transparent)]
pub(crate) struct SyncMethods(pub ffi::sqlite3_pcache_methods2);

// SAFETY: PCACHE2_METHODS' fields are read-only after process start
// (pArg is null; all xFn pointers are immutable function pointers);
// SQLite reads the struct from multiple threads but never writes.
unsafe impl Sync for SyncMethods {}

pub(crate) static PCACHE2_METHODS: SyncMethods = SyncMethods(ffi::sqlite3_pcache_methods2 {
    iVersion: 1,
    pArg: std::ptr::null_mut(),
    xInit: Some(pcache_init),
    xShutdown: Some(pcache_shutdown),
    xCreate: Some(pcache_create),
    xCachesize: Some(pcache_cachesize),
    xPagecount: Some(pcache_pagecount),
    xFetch: Some(pcache_fetch),
    xUnpin: Some(pcache_unpin),
    xRekey: Some(pcache_rekey),
    xTruncate: Some(pcache_truncate),
    xDestroy: Some(pcache_destroy),
    xShrink: Some(pcache_shrink),
});
