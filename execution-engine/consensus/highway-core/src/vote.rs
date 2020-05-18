use derive_more::Deref;

use crate::{state::State, traits::Context, validators::ValidatorIndex, vertex::WireVote};

/// The observed behavior of a validator at some point in time.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Observation<C: Context> {
    /// No vote by that validator was observed yet.
    None,
    /// The validator's latest vote.
    Correct(C::VoteHash),
    /// The validator has been seen
    Faulty,
}

impl<C: Context> Observation<C> {
    /// Returns the vote hash, if this is a correct observation.
    pub fn correct(&self) -> Option<&C::VoteHash> {
        match self {
            Self::None | Self::Faulty => None,
            Self::Correct(hash) => Some(hash),
        }
    }

    fn is_correct(&self) -> bool {
        match self {
            Self::None | Self::Faulty => false,
            Self::Correct(_) => true,
        }
    }
}

/// The observed behavior of all validators at some point in time.
#[derive(Clone, Debug, Deref, Eq, PartialEq)]
pub struct Panorama<C: Context>(pub Vec<Observation<C>>);

impl<C: Context> Panorama<C> {
    /// Creates a new, empty panorama.
    pub fn new(num_validators: usize) -> Panorama<C> {
        Panorama(vec![Observation::None; num_validators])
    }

    /// Returns the observation for the given validator. Panics if the index is out of range.
    pub fn get(&self, idx: ValidatorIndex) -> &Observation<C> {
        &self[usize::from(idx.0)]
    }

    /// Returns `true` if there is no correct observation yet.
    pub fn is_empty(&self) -> bool {
        !self.iter().any(Observation::is_correct)
    }

    /// Returns an iterator over all observations, by validator index.
    pub fn enumerate(&self) -> impl Iterator<Item = (ValidatorIndex, &Observation<C>)> {
        self.iter()
            .enumerate()
            .map(|(idx, obs)| (ValidatorIndex(idx as u16), obs))
    }

    /// Updates this panorama by adding one vote. Assumes that all justifications of that vote are
    /// already seen.
    pub fn update(&mut self, idx: ValidatorIndex, obs: Observation<C>) {
        self.0[usize::from(idx.0)] = obs;
    }
}

/// A vote sent to or received from the network.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Vote<C: Context> {
    // TODO: Signature
    /// The list of latest messages and faults observed by the sender of this message.
    pub panorama: Panorama<C>,
    /// The number of earlier messages by the same sender.
    pub seq_number: u64,
    /// The validator who created and sent this vote.
    pub sender: ValidatorIndex,
    /// The block this is a vote for. Either it or its parent must be the fork choice.
    pub block: C::VoteHash,
    /// A skip list index of the sender's swimlane, i.e. the previous vote by the same sender.
    ///
    /// For every `p = 1 << i` that divides `seq_number`, this contains an `i`-th entry pointing to
    /// the older vote with `seq_number - p`.
    pub skip_idx: Vec<C::VoteHash>,
}

impl<C: Context> Vote<C> {
    /// Creates a new `Vote` from the `WireVote`, and returns the values if it contained any.
    /// Values must be stored as a block, with the same hash.
    pub fn new(
        wvote: WireVote<C>,
        fork_choice: Option<&C::VoteHash>,
        state: &State<C>,
    ) -> (Vote<C>, Option<Vec<C::ConsensusValue>>) {
        let block = if wvote.values.is_some() {
            wvote.hash // A vote with a new block votes for itself.
        } else {
            // If the vote didn't introduce a new block, it votes for the fork choice itself.
            // `Highway::add_vote` checks that the panorama is not empty.
            fork_choice
                .cloned()
                .expect("nonempty panorama has nonempty fork choice")
        };
        let mut skip_idx = Vec::new();
        if let Some(hash) = wvote.panorama.get(wvote.sender).correct() {
            skip_idx.push(hash.clone());
            for i in 0..wvote.seq_number.trailing_zeros() as usize {
                let old_vote = state.vote(&skip_idx[i]);
                skip_idx.push(old_vote.skip_idx[i].clone());
            }
        }
        let vote = Vote {
            panorama: wvote.panorama,
            seq_number: wvote.seq_number,
            sender: wvote.sender,
            block,
            skip_idx,
        };
        (vote, wvote.values)
    }
}
