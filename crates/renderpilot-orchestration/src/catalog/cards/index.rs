use std::collections::HashMap;
use std::sync::Arc;

use super::GameCardData;

pub(super) fn build_string_index<'a, I>(
    cards: &'a [Arc<GameCardData>],
    keys: impl Fn(&'a GameCardData) -> I,
) -> HashMap<String, Arc<[usize]>>
where
    I: Iterator<Item = &'a str>,
{
    let mut index = HashMap::<String, Vec<usize>>::new();
    for (card_index, card) in cards.iter().enumerate() {
        for key in keys(card) {
            index.entry(key.to_owned()).or_default().push(card_index);
        }
    }
    index
        .into_iter()
        .map(|(key, indices)| (key, Arc::from(indices)))
        .collect()
}

pub(super) fn collect_index_union(
    index: &HashMap<String, Arc<[usize]>>,
    keys: &[String],
) -> Option<Vec<usize>> {
    if keys.is_empty() {
        return None;
    }
    let mut indices = keys
        .iter()
        .filter_map(|key| index.get(key))
        .flat_map(|values| values.iter().copied())
        .collect::<Vec<_>>();
    indices.sort_unstable();
    indices.dedup();
    Some(indices)
}

pub(super) fn intersect_sorted(left: &[usize], right: &[usize]) -> Vec<usize> {
    let (mut left_index, mut right_index) = (0, 0);
    let mut intersection = Vec::with_capacity(left.len().min(right.len()));
    while left_index < left.len() && right_index < right.len() {
        match left[left_index].cmp(&right[right_index]) {
            std::cmp::Ordering::Less => left_index += 1,
            std::cmp::Ordering::Greater => right_index += 1,
            std::cmp::Ordering::Equal => {
                intersection.push(left[left_index]);
                left_index += 1;
                right_index += 1;
            }
        }
    }
    intersection
}
