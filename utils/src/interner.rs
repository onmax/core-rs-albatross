use std::{
    borrow::Borrow,
    collections::HashSet,
    hash::{Hash, Hasher},
    sync::{Arc, Weak},
};

use parking_lot::RwLock;

/// Makes sure there's at most one instance of an object `V` per key `K`.
pub struct Interner<K: Clone + Eq + Hash, V>(Arc<RwLock<InternerInner<K, V>>>);

struct InternerInner<K: Clone + Eq + Hash, V> {
    cache: HashSet<Interned<K, V>>,
}

/// An interned value together with its key.
pub struct Interned<K: Clone + Eq + Hash, V>(Arc<InternedInner<K, V>>);

struct InternedInner<K: Clone + Eq + Hash, V> {
    key: K,
    value: V,
    interner: Weak<RwLock<InternerInner<K, V>>>,
}

impl<K: Clone + Eq + Hash, V> Interned<K, V> {
    pub fn key(&self) -> &K {
        &self.0.key
    }
    pub fn value(&self) -> &V {
        &self.0.value
    }
    #[cfg(test)]
    fn strong_count(this: &Interned<K, V>) -> usize {
        Arc::strong_count(&this.0)
    }
    #[cfg(test)]
    fn ptr_eq(this: &Interned<K, V>, other: &Interned<K, V>) -> bool {
        Arc::ptr_eq(&this.0, &other.0)
    }
}

impl<K: Clone + Eq + Hash, V> Borrow<K> for Interned<K, V> {
    fn borrow(&self) -> &K {
        self.key()
    }
}

impl<K: Clone + Eq + Hash, V> Clone for Interned<K, V> {
    fn clone(&self) -> Interned<K, V> {
        Interned(Arc::clone(&self.0))
    }
}

impl<K: Clone + Eq + Hash, V> Eq for Interned<K, V> {}

impl<K: Clone + Eq + Hash, V> Hash for Interned<K, V> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.key().hash(state)
    }
}

impl<K: Clone + Eq + Hash, V> PartialEq for Interned<K, V> {
    fn eq(&self, other: &Self) -> bool {
        self.key().eq(other.key())
    }
}

impl<K: Clone + Eq + Hash, V> Default for Interner<K, V> {
    fn default() -> Interner<K, V> {
        Interner(Arc::new(RwLock::new(InternerInner {
            cache: Default::default(),
        })))
    }
}

impl<K: Clone + Eq + Hash, V> Interner<K, V> {
    pub fn intern_with<F: FnOnce() -> V>(&self, key: &K, value: F) -> Interned<K, V> {
        // Fast path: the key is already in the cache.
        if let Some(interned) = self.0.read().cache.get(key) {
            return interned.clone();
        }
        // Slow path: we need to acquire a write lock for the cache.
        let cache = &mut self.0.write().cache;
        // We need to re-check whether the key got added already since we
        // dropped the lock.
        if let Some(interned) = cache.get(key) {
            return interned.clone();
        }
        // If it's not in the cache yet, we need to create a new instance.
        let interned = Interned(Arc::new(InternedInner {
            key: key.clone(),
            value: value(),
            interner: Arc::downgrade(&self.0),
        }));
        assert!(cache.insert(interned.clone()));
        interned
    }
    pub fn intern(&self, key: &K, value: V) -> Interned<K, V> {
        self.intern_with(key, || value)
    }
}

impl<K: Clone + Eq + Hash, V> Drop for Interned<K, V> {
    fn drop(&mut self) {
        // If the strong count is exactly two, it means that there is the copy
        // in the cache and the copy which is being dropped right now.
        if Arc::strong_count(&self.0) != 2 {
            return;
        }
        // If the relevant interner is already gone, just ignore the message.
        if let Some(inner) = self.0.interner.upgrade() {
            let mut inner = inner.write();

            // We need to check the strong count again, as there might have
            // been calls to the interner from before we took the lock.
            if Arc::strong_count(&self.0) != 2 {
                return;
            }

            // If the interner doesn't have the key anymore, we might be in a
            // recursive drop call since we also drop an interned value in this
            // function.
            //
            // Do nothing in this case.
            let Some(entry) = inner.cache.take(self.key()) else {
                return;
            };

            // This should point to the same thing as we do.
            assert!(Arc::ptr_eq(&self.0, &entry.0));

            // We need to drop the lock first because dropping `entry` is going
            // to cause a recursive call.
            drop(inner);
            drop(entry);
        }
    }
}

#[cfg(test)]
mod test {
    use super::{Interned, Interner};

    #[test]
    fn interner_smoke_test() {
        let interner = Interner::default();

        let interned_0 = interner.intern(&(), 0);
        let interned_1 = interner.intern(&(), 1);
        assert_eq!(*interned_0.value(), 0);
        assert_eq!(*interned_1.value(), 0);
        assert!(Interned::ptr_eq(&interned_0, &interned_1));
    }

    #[test]
    fn interner_drop() {
        let interner = Interner::default();
        let interned = interner.intern(&(), ());

        drop(interner);
        assert_eq!(Interned::strong_count(&interned), 1);
        drop(interned);
    }
}
