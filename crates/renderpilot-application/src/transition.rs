//! Pure, complete resolution of a component replacement transition.

mod model;
mod paths;
mod projection;
mod resolver;
mod xiph;

#[cfg(test)]
mod tests;

pub use model::{
    ArchiveMode, ExternalAliasRequirements, ResolvedArchiveAndRemove, ResolvedPathDisposition,
    ResolvedRemove, ResolvedTransition, ResolvedUntouchedBaseline, ResolvedWrite,
    ResolvedXiphTransition,
};
pub use projection::{
    resolve_transition_install_target, resolve_transition_members, resolve_transition_removals,
};
pub use resolver::resolve_transition;
