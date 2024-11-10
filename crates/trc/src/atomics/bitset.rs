/*
 * SPDX-FileCopyrightText: 2020 Stalwart Labs Ltd <hello@stalw.art>
 *
 * SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-SEL
 */

use std::sync::atomic::{AtomicUsize, Ordering};

use crate::ipc::{bitset::Bitset, USIZE_BITS, USIZE_BITS_MASK};

pub struct AtomicBitset<const N: usize>([AtomicUsize; N]);

impl<const N: usize> AtomicBitset<N> {
    #[allow(clippy::new_without_default)]
    #[allow(clippy::declare_interior_mutable_const)]
    pub const fn new() -> Self {
        Self({
            const INIT: AtomicUsize = AtomicUsize::new(0);
            let mut array = [INIT; N];
            let mut i = 0;
            while i < N {
                array[i] = AtomicUsize::new(0);
                i += 1;
            }
            array
        })
    }

    #[inline(always)]
    pub fn set(&self, index: impl Into<usize>) {
        let index = index.into();
        self.0[index / USIZE_BITS].fetch_or(1 << (index & USIZE_BITS_MASK), Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn clear(&self, index: impl Into<usize>) {
        let index = index.into();
        self.0[index / USIZE_BITS].fetch_and(!(1 << (index & USIZE_BITS_MASK)), Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn get(&self, index: impl Into<usize>) -> bool {
        let index = index.into();
        self.0[index / USIZE_BITS].load(Ordering::Relaxed) & (1 << (index & USIZE_BITS_MASK)) != 0
    }

    pub fn update(&self, bitset: impl AsRef<Bitset<N>>) {
        let bitset = bitset.as_ref();
        for i in 0..N {
            self.0[i].store(bitset.0[i], Ordering::Relaxed);
        }
    }

    pub fn union(&self, bitset: impl AsRef<Bitset<N>>) {
        let bitset = bitset.as_ref();
        for i in 0..N {
            self.0[i].fetch_or(bitset.0[i], Ordering::Relaxed);
        }
    }

    pub fn clear_all(&self) {
        for i in 0..N {
            self.0[i].store(0, Ordering::Relaxed);
        }
    }

    pub fn is_empty(&self) -> bool {
        for i in 0..N {
            if self.0[i].load(Ordering::Relaxed) != 0 {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SIZE: usize = 1000;
    type TestBitset = AtomicBitset<{ (TEST_SIZE + USIZE_BITS - 1) / USIZE_BITS }>;
    static BITSET: TestBitset = TestBitset::new();

    #[test]
    fn test_atomic_bitset() {
        for i in 0..TEST_SIZE {
            assert!(!BITSET.get(i), "Bit {i} should be unset in new BITSET");
        }

        for i in 0..TEST_SIZE {
            assert!(!BITSET.get(i), "Bit {i} should be initially unset");
            BITSET.set(i);
            assert!(BITSET.get(i), "Bit {i} should be set after setting");
        }

        BITSET.clear_all();

        for i in 0..TEST_SIZE {
            BITSET.set(i);
            assert!(BITSET.get(i), "Bit {i} should be set before clearing");
            BITSET.clear(i);
            assert!(!BITSET.get(i), "Bit {i} should be unset after clearing");
        }

        BITSET.clear_all();

        // Set even bits
        for i in (0..TEST_SIZE).step_by(2) {
            BITSET.set(i);
        }

        // Check all bits
        for i in 0..TEST_SIZE {
            if i % 2 == 0 {
                assert!(BITSET.get(i), "Even bit {i} should be set");
            } else {
                assert!(!BITSET.get(i), "Odd bit {i} should be unset");
            }
        }

        // Clear even bits and set odd bits
        for i in 0..TEST_SIZE {
            if i % 2 == 0 {
                BITSET.clear(i);
            } else {
                BITSET.set(i);
            }
        }

        // Check all bits again
        for i in 0..TEST_SIZE {
            if i % 2 == 0 {
                assert!(!BITSET.get(i), "Even bit {i} should now be unset");
            } else {
                assert!(BITSET.get(i), "Odd bit {i} should now be set");
            }
        }
    }
}
