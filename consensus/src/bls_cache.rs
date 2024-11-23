use nimiq_bls::LazyPublicKey as BlsLazyPublicKey;
use nimiq_primitives::policy::Policy;

// TODO: implement some caching strategy
pub struct BlsCache;

impl Default for BlsCache {
    fn default() -> BlsCache {
        BlsCache::with_capacity(Policy::BLS_CACHE_MAX_CAPACITY)
    }
}

impl BlsCache {
    fn with_capacity(capacity: usize) -> BlsCache {
        let _ = capacity;
        BlsCache
    }
    pub fn new_test() -> BlsCache {
        BlsCache::with_capacity(100)
    }
}

impl BlsCache {
    pub fn cache(&mut self, _data: &BlsLazyPublicKey) {}
}
