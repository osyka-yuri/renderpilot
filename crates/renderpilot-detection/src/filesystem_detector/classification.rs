//! Maps a matched library pattern to the UI-facing classification fields:
//! component kind, detection confidence, and swappability.

use renderpilot_domain::{ComponentKind, LibraryTechnology, Swappability};

use crate::PatternKind;

use super::DetectionConfidence;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct LibraryFileClassification {
    pub(super) technology: LibraryTechnology,
    pub(super) kind: ComponentKind,
    pub(super) confidence: DetectionConfidence,
    pub(super) swappability: Swappability,
}

impl LibraryFileClassification {
    pub(super) fn new(technology: LibraryTechnology, pattern_kind: PatternKind) -> Self {
        Self {
            technology,
            kind: component_kind_for(technology),
            confidence: confidence_for(pattern_kind, technology),
            swappability: swappability_for(technology),
        }
    }
}

fn component_kind_for(technology: LibraryTechnology) -> ComponentKind {
    match technology {
        LibraryTechnology::NvidiaStreamline => ComponentKind::StreamlineComponent,
        _ => ComponentKind::NativeLibrary,
    }
}

fn confidence_for(pattern_kind: PatternKind, technology: LibraryTechnology) -> DetectionConfidence {
    match (pattern_kind, technology) {
        (_, LibraryTechnology::Unknown) => DetectionConfidence::Low,
        (PatternKind::Exact, _) => DetectionConfidence::High,
        (PatternKind::Glob, _) => DetectionConfidence::Medium,
    }
}

fn swappability_for(technology: LibraryTechnology) -> Swappability {
    match technology {
        LibraryTechnology::NvidiaStreamline => Swappability::BundleOnly,
        LibraryTechnology::Unknown => Swappability::Unknown,
        _ => Swappability::Swappable,
    }
}
