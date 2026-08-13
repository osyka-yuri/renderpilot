//! Private durable outbox hub. Each child owns one cohesive record family or
//! pure proof; this module exports the bounded internal collaboration surface.

mod append_intent;
mod observation;
mod repair;
mod store;
mod target;

pub(super) use append_intent::{
    AppendIntentV1, AuthenticatedAppendIntent, append_intent_key, intended_entry_is_valid,
    intent_is_exact_aborted_origin, new_append_intent, read_append_intents, record_append_intent,
};
pub(super) use observation::{
    ObservationOutcome, observation_key, observation_matches, observe_exact_current,
};
pub(super) use repair::{
    record_repair_intent, repair_intent_key, retained_repair_matches, tail_repair_replay_subject,
    truncate_exact_tail,
};
#[cfg(test)]
pub(super) use target::target_prefix_len_for_test;
pub(super) use target::{
    matches_committed_target, matches_exact_committed_target, target_prefix_len,
};
