//! One stable sort for the comparison sorts of core. Each call of a
//! generic sort compiles its own copy of the sort, so these callers sort
//! positions through one function that is not generic.

use std::cmp::Ordering;

/// The positions `0..len` in the order of `compare`. Positions that
/// compare equal keep their order.
#[inline(never)]
pub(crate) fn stable_order(len: usize, compare: &dyn Fn(usize, usize) -> Ordering) -> Vec<usize> {
    let mut order: Vec<usize> = (0..len).collect();
    order.sort_by(|a, b| compare(*a, *b));
    order
}
