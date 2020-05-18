use std::{
    collections::BTreeMap,
    iter::{self, Extend, FromIterator},
};

use derive_more::{Deref, DerefMut};

use crate::{state::State, traits::Context};

/// A tally of votes at a specific height. This is never empty: It contains at least one vote.
#[derive(Clone)]
pub struct Tally<'a, C: Context> {
    /// The block with the highest weight, and the highest hash if there's a tie.
    max: (u64, &'a C::VoteHash),
    /// The total vote weight for each block.
    votes: BTreeMap<&'a C::VoteHash, u64>,
}

impl<'a, C: Context> Extend<(&'a C::VoteHash, u64)> for Tally<'a, C> {
    fn extend<T: IntoIterator<Item = (&'a C::VoteHash, u64)>>(&mut self, iter: T) {
        for (bhash, w) in iter {
            self.add(bhash, w);
        }
    }
}

impl<'a, 'b, C: Context> IntoIterator for &'b Tally<'a, C> {
    type Item = (&'a C::VoteHash, u64);
    type IntoIter = Box<dyn Iterator<Item = Self::Item> + 'b>;

    fn into_iter(self) -> Self::IntoIter {
        Box::new(self.votes.iter().map(|(b, w)| (*b, *w)))
    }
}

impl<'a, C: Context> Tally<'a, C> {
    /// Returns a new tally with a single entry.
    fn new(bhash: &'a C::VoteHash, w: u64) -> Self {
        Tally {
            max: (w, bhash),
            votes: iter::once((bhash, w)).collect(),
        }
    }

    /// Creates a tally from a list of votes. Returns `None` if the iterator is empty.
    fn try_from_iter<T: IntoIterator<Item = (&'a C::VoteHash, u64)>>(iter: T) -> Option<Self> {
        let mut iter = iter.into_iter();
        let (bhash, w) = iter.next()?;
        let mut tally = Tally::new(bhash, w);
        tally.extend(iter);
        Some(tally)
    }

    /// Returns a new tally with the same votes, but one level lower: vote for a block counts as a
    /// vote for that block's parent. Panics if called on level 0.
    fn parents(&self, state: &'a State<C>) -> Self {
        let to_parent = |(h, w): (&&'a C::VoteHash, &u64)| (state.block(*h).parent().unwrap(), *w);
        Self::try_from_iter(self.votes.iter().map(to_parent)).unwrap()
    }

    /// Adds a vote for a block to the tally, possibly updating the current maximum.
    fn add(&mut self, bhash: &'a C::VoteHash, weight: u64) {
        let w = self.votes.entry(bhash).or_default();
        *w += weight;
        self.max = (*w, bhash).max(self.max);
    }

    /// Returns the total weight of the votes included in this tally.
    fn weight(&self) -> u64 {
        self.votes.values().sum()
    }

    /// Returns the maximum voting weight a single block received.
    fn max_w(&self) -> u64 {
        self.max.0
    }

    /// Returns the block hash that received the most votes; the highest hash in case of a tie.
    fn max_bhash(&self) -> &'a C::VoteHash {
        self.max.1
    }

    /// Returns a tally containing only the votes for descendants of `bhash`.
    fn filter(self, height: u64, bhash: &'a C::VoteHash, state: &'a State<C>) -> Option<Self> {
        let iter = self.votes.into_iter();
        Self::try_from_iter(iter.filter(|&(b, _)| state.find_ancestor(b, height) == Some(bhash)))
    }
}

/// A list of tallies by block height. The tally at each height contains only the votes that point
/// directly to a block at that height, not at a descendant.
#[derive(Deref, DerefMut)]
pub struct Tallies<'a, C: Context>(BTreeMap<u64, Tally<'a, C>>);

impl<'a, C: Context> Default for Tallies<'a, C> {
    fn default() -> Self {
        Tallies(BTreeMap::new())
    }
}

impl<'a, C: Context> FromIterator<(u64, &'a C::VoteHash, u64)> for Tallies<'a, C> {
    fn from_iter<T: IntoIterator<Item = (u64, &'a C::VoteHash, u64)>>(iter: T) -> Self {
        let mut tallies = Self::default();
        for (height, bhash, weight) in iter {
            tallies.add(height, bhash, weight);
        }
        tallies
    }
}

impl<'a, C: Context> Tallies<'a, C> {
    /// Returns the height and hash of a block that is an ancestor of the fork choice, and _not_ an
    /// ancestor of all entries in `self`. Returns `None` if `self` is empty.
    pub fn find_decided(&self, state: &'a State<C>) -> Option<(u64, &'a C::VoteHash)> {
        let max_height = *self.keys().next_back()?;
        let total_weight = self.values().map(Tally::weight).sum();
        // In the loop, this will be the tally of all votes from higher than the current height.
        let mut prev_tally = self[&max_height].clone();
        // Start from `max_height - 1` and find the greatest height where a decision can be made.
        for height in (0..max_height).rev() {
            // The tally at `height` is the sum of the parents of `prev_height` and the votes that
            // point directly to blocks at `height`.
            let mut h_tally = prev_tally.parents(state);
            if let Some(tally) = self.get(&height) {
                h_tally.extend(tally);
            }
            // If any block received more than 50%, a decision can be made: Either that block is
            // the fork choice, or we can pick its highest scoring child from `prev_tally`.
            if h_tally.max_w() * 2 > total_weight {
                return Some(
                    match prev_tally.filter(height, h_tally.max_bhash(), state) {
                        Some(filtered) => (height + 1, filtered.max_bhash()),
                        None => (height, h_tally.max_bhash()),
                    },
                );
            }
            prev_tally = h_tally;
        }
        // Even at level 0 no block received a majority. Pick the one with the highest weight.
        Some((0, prev_tally.max_bhash()))
    }

    /// Removes all votes for blocks that are not descendants of `bhash`.
    pub fn filter(self, height: u64, bhash: &'a C::VoteHash, state: &'a State<C>) -> Self {
        // Each tally will be filtered to remove blocks incompatible with `bhash`.
        let map_compatible =
            |(h, t): (u64, Tally<'a, C>)| t.filter(height, bhash, state).map(|t| (h, t));
        // All tallies at `height` and lower can be removed, too.
        let relevant_heights = self.0.into_iter().rev().take_while(|(h, _)| *h > height);
        Tallies(relevant_heights.filter_map(map_compatible).collect())
    }

    /// Adds an entry to the tally at the specified `height`.
    fn add(&mut self, height: u64, bhash: &'a C::VoteHash, weight: u64) {
        self.entry(height)
            .and_modify(|tally| tally.add(bhash, weight))
            .or_insert_with(|| Tally::new(bhash, weight));
    }
}
