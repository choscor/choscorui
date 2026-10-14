//! A hash map with the address of a native object as the key.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

/// Hashes an object address with one multiply. The keys are addresses of
/// live objects, so the maps do not need the flooding resistance of the
/// default hasher, and its code in each lookup.
#[derive(Default)]
pub(crate) struct AddressHasher(u64);

impl Hasher for AddressHasher {
    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.write_u64(u64::from(*byte));
        }
    }

    fn write_u64(&mut self, value: u64) {
        self.0 = (self.0.rotate_left(5) ^ value).wrapping_mul(0x517c_c1b7_2722_0a95);
    }

    fn write_usize(&mut self, value: usize) {
        self.write_u64(value as u64);
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

/// A map with an object address as the key.
pub(crate) type AddressMap<V> = HashMap<usize, V, BuildHasherDefault<AddressHasher>>;
