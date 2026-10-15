//! One stable sort for the sorts of the controls. Each call of a generic
//! sort compiles its own copy of the sort, so the controls sort positions
//! through one function that is not generic.

use std::cmp::Ordering;

/// Sorts `items` by `compare`. Items that compare equal keep their order.
pub(crate) fn sort_by<T>(items: &mut Vec<T>, compare: impl Fn(&T, &T) -> Ordering) {
    let order = stable_order(items.len(), &|a, b| compare(&items[a], &items[b]));
    let mut slots: Vec<Option<T>> = items.drain(..).map(Some).collect();
    items.extend(order.into_iter().filter_map(|index| slots[index].take()));
}

/// The positions `0..len` in the order of `compare`. Positions that
/// compare equal keep their order.
#[inline(never)]
fn stable_order(len: usize, compare: &dyn Fn(usize, usize) -> Ordering) -> Vec<usize> {
    let mut order: Vec<usize> = (0..len).collect();
    order.sort_by(|a, b| compare(*a, *b));
    order
}
