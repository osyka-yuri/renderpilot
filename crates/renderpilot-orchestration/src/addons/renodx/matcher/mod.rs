//! Deterministic, explainable resolver from an installed game to a RenoDX outcome.
//!
//! [`MatchFacts`] about a game are evaluated against every [`Title`]'s ordered,
//! tiered rules; the highest-specificity match wins by a documented tie-break. The
//! winner is then routed by its category (external / native-HDR / blacklist) or,
//! for a standard install, gated by its compatibility constraints. When no per-game
//! title matches, an **engine-generic** fallback is tried from the detected engine.
//! The resolution is owned (it clones the few fields the install needs) so
//! downstream layers carry no manifest borrow.

mod plan;
mod types;

// The tool-agnostic matching vocabulary (facts, confidence, incompatibility, the
// per-rule matcher) is shared; re-exported so the RenoDX subsystem keeps
// addressing it as `matcher::MatchFacts` etc.
pub use crate::addons::matching::{IncompatibilityReason, MatchConfidence, MatchFacts};
pub use plan::{
    file_installable, generic_file_install_plan, matched_slug, resolve, resolve_external_install,
};
pub use types::{RenoDxResolution, ResolvedInstall};

use super::types::{MatchRule, Title};
use crate::addons::matching::SelectableTitle;

impl SelectableTitle for Title {
    fn match_rules(&self) -> &[MatchRule] {
        &self.match_rules
    }

    fn channel(&self) -> crate::addons::matching::Channel {
        self.channel
    }

    fn id(&self) -> &str {
        &self.id
    }
}

#[cfg(test)]
mod tests;
