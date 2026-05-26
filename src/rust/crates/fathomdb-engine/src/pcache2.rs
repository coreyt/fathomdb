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

use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::collections::{HashMap, HashSet};
use std::os::raw::{c_int, c_uint, c_void};
use std::sync::Mutex;

use rusqlite::ffi;

/// Alignment used for both pBuf and pExtra allocations. SQLite stores
/// a `PgHdr1`-style C struct in pExtra; on x86_64 the natural
/// alignment is 8 bytes. We allocate both regions through the global
/// allocator at this alignment so writes through pExtra cannot trip
/// unaligned-access UB.
const PAGE_ALIGN: usize = 16;

// Each cache instance owns a Mutex<State> guarding its hashmap.
struct Cache {
    sz_page: c_int,
    sz_extra: c_int,
    purgeable: bool,
    state: Mutex<State>,
}

struct State {
    pages: HashMap<c_uint, Box<Page>>,
    /// Keys of currently-unpinned pages (eviction candidates). Not
    /// LRU-ordered — the prototype only needs O(1) membership and
    /// "pick one to evict", not strict recency. SQLite already gives
    /// us cache-size hints so wrong-page eviction is bounded.
    unpinned: HashSet<c_uint>,
    /// Soft limit on the number of pages SQLite asked us to retain.
    /// `0` means unlimited.
    cache_size_hint: c_int,
}

struct Page {
    /// Embedded handle returned to SQLite from xFetch. SQLite uses
    /// this pointer as the page handle and expects the same address
    /// back on repeated fetches of the same key while pinned, so
    /// this field lives inside the Page (which is heap-allocated and
    /// never moves) and its address is what we hand out.
    handle: ffi::sqlite3_pcache_page,
    /// pBuf backing: `sz_page` bytes, zeroed, aligned to PAGE_ALIGN.
    buf_ptr: *mut u8,
    /// pExtra backing: `sz_extra` bytes, zeroed, aligned to
    /// PAGE_ALIGN. When `sz_extra == 0`, allocated as 1 byte so the
    /// pointer is non-null and distinct from buf_ptr.
    extra_ptr: *mut u8,
    sz_page: usize,
    sz_extra: usize,
    /// SQLite page number for this entry. Stored so xUnpin can
    /// recover the key from the handle pointer in O(1).
    key: c_uint,
    pinned: bool,
}

// SAFETY: Page owns two heap allocations via raw pointers but never
// shares them across threads except through the Mutex<State> on the
// owning Cache. Sending Page across threads moves both the struct
// and the pointed-to memory ownership together.
unsafe impl Send for Page {}

impl Page {
    fn new(sz_page: usize, sz_extra: usize, key: c_uint) -> Box<Page> {
        debug_assert!(
            sz_page.is_multiple_of(PAGE_ALIGN),
            "sz_page {sz_page} not aligned to {PAGE_ALIGN}"
        );
        // SAFETY: Layout sizes are non-zero (sz_page is at least the
        // SQLite page size; sz_extra clamped to >=1). alloc_zeroed
        // returns a pointer aligned to the requested alignment or
        // null on failure; we assert non-null because OOM at this
        // path would corrupt SQLite anyway.
        unsafe {
            let buf_layout = Layout::from_size_align(sz_page, PAGE_ALIGN).expect("buf layout");
            let buf_ptr = alloc_zeroed(buf_layout);
            assert!(!buf_ptr.is_null(), "pcache2: buf allocation failed");
            let extra_sz = sz_extra.max(1);
            let extra_layout = Layout::from_size_align(extra_sz, PAGE_ALIGN).expect("extra layout");
            let extra_ptr = alloc_zeroed(extra_layout);
            assert!(!extra_ptr.is_null(), "pcache2: extra allocation failed");
            Box::new(Page {
                handle: ffi::sqlite3_pcache_page {
                    pBuf: buf_ptr.cast::<c_void>(),
                    pExtra: extra_ptr.cast::<c_void>(),
                },
                buf_ptr,
                extra_ptr,
                sz_page,
                sz_extra,
                key,
                pinned: true,
            })
        }
    }

    fn handle_ptr(&mut self) -> *mut ffi::sqlite3_pcache_page {
        &mut self.handle as *mut _
    }

    /// Recover the owning Page from a handle pointer that we
    /// previously returned to SQLite from xFetch. SQLite hands the
    /// same pointer back to xUnpin / xRekey, and since the handle
    /// is an inline field, subtracting `offset_of!(Page, handle)`
    /// gives the Page address.
    ///
    /// SAFETY: caller must ensure `handle` is a non-null pointer
    /// returned by this module's xFetch and still owned by the
    /// caching cache instance (i.e. not freed by a prior discarding
    /// unpin).
    unsafe fn from_handle_ptr<'a>(handle: *mut ffi::sqlite3_pcache_page) -> &'a Page {
        debug_assert!(!handle.is_null());
        let off = std::mem::offset_of!(Page, handle);
        &*(handle as *mut u8).sub(off).cast::<Page>()
    }
}

impl Drop for Page {
    fn drop(&mut self) {
        // SAFETY: pointers were obtained from alloc_zeroed with the
        // matching Layout below.
        unsafe {
            if !self.buf_ptr.is_null() {
                let layout = Layout::from_size_align_unchecked(self.sz_page, PAGE_ALIGN);
                dealloc(self.buf_ptr, layout);
            }
            if !self.extra_ptr.is_null() {
                let layout = Layout::from_size_align_unchecked(self.sz_extra.max(1), PAGE_ALIGN);
                dealloc(self.extra_ptr, layout);
            }
        }
    }
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
        state: Mutex::new(State {
            pages: HashMap::new(),
            unpinned: HashSet::new(),
            cache_size_hint: 0,
        }),
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
    debug_assert!(
        (cache_ref.sz_page as usize).is_multiple_of(PAGE_ALIGN),
        "SQLite passed sz_page={} not aligned to {PAGE_ALIGN}",
        cache_ref.sz_page,
    );
    let mut s = cache_ref.state.lock().unwrap();

    if let Some(page) = s.pages.get_mut(&key) {
        page.pinned = true;
        let ptr = page.handle_ptr();
        s.unpinned.remove(&key);
        return ptr;
    }

    if create_flag == 0 {
        return std::ptr::null_mut();
    }

    // Evict if at capacity (best-effort; not LRU-ordered).
    if cache_ref.purgeable && s.cache_size_hint > 0 {
        let limit = s.cache_size_hint as usize;
        while s.pages.len() >= limit {
            let evict = match s.unpinned.iter().next().copied() {
                Some(k) => k,
                None => break, // no unpinned page available
            };
            s.unpinned.remove(&evict);
            s.pages.remove(&evict);
        }
    }

    let mut page = Page::new(cache_ref.sz_page as usize, cache_ref.sz_extra as usize, key);
    let ptr = page.handle_ptr();
    s.pages.insert(key, page);
    ptr
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
    // Recover the key from the handle pointer in O(1): the handle
    // is an inline field of the Page, so subtracting its offset
    // gives the Page, which carries its own `key`.
    let key = Page::from_handle_ptr(page_ptr).key;

    let mut s = cache_ref.state.lock().unwrap();
    if discard != 0 {
        s.pages.remove(&key);
        s.unpinned.remove(&key);
    } else if let Some(p) = s.pages.get_mut(&key) {
        p.pinned = false;
        s.unpinned.insert(key);
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
    if let Some(mut page) = s.pages.remove(&old_key) {
        page.key = new_key;
        s.pages.insert(new_key, page);
    }
    if s.unpinned.remove(&old_key) {
        s.unpinned.insert(new_key);
    }
}

unsafe extern "C" fn pcache_truncate(cache: *mut ffi::sqlite3_pcache, i_limit: c_uint) {
    let cache_ref: &Cache = &*cache.cast::<Cache>();
    let mut s = cache_ref.state.lock().unwrap();
    let drop_keys: Vec<c_uint> = s.pages.keys().copied().filter(|k| *k >= i_limit).collect();
    for k in drop_keys {
        s.pages.remove(&k);
        s.unpinned.remove(&k);
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
    let to_evict: Vec<c_uint> = s.unpinned.drain().collect();
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
