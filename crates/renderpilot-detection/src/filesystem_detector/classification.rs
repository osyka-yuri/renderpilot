//! Maps a matched library pattern to the UI-facing classification fields:
//! component kind, detection confidence, and swappability.

use renderpilot_domain::{ComponentKind, GraphicsTechnology, Swappability};

use crate::PatternKind;

use super::DetectionConfidence;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct LibraryFileClassification {
    pub(super) technology: GraphicsTechnology,
    pub(super) kind: ComponentKind,
    pub(super) confidence: DetectionConfidence,
    pub(super) swappability: Swappability,
}

impl LibraryFileClassification {
    pub(super) fn new(technology: GraphicsTechnology, pattern_kind: PatternKind) -> Self {
        Self {
            technology,
            kind: component_kind_for(technology),
            confidence: confidence_for(pattern_kind, technology),
            swappability: swappability_for(technology),
        }
    }
}

fn component_kind_for(technology: GraphicsTechnology) -> ComponentKind {
    match technology {
        GraphicsTechnology::NvidiaStreamline => ComponentKind::StreamlineComponent,
        _ => ComponentKind::NativeLibrary,
    }
}

fn confidence_for(
    pattern_kind: PatternKind,
    technology: GraphicsTechnology,
) -> DetectionConfidence {
    match (pattern_kind, technology) {
        (_, GraphicsTechnology::Unknown) => DetectionConfidence::Low,
        (PatternKind::Exact, _) => DetectionConfidence::High,
        (PatternKind::Glob, _) => DetectionConfidence::Medium,
    }
}

fn swappability_for(technology: GraphicsTechnology) -> Swappability {
    match technology {
        GraphicsTechnology::NvidiaStreamline => Swappability::BundleOnly,
        GraphicsTechnology::Unknown => Swappability::Unknown,
        _ => Swappability::Swappable,
    }
}
