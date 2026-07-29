#[derive(Clone, Copy)]
pub(super) enum DetectionMode {
    /// Full filesystem pass, but reuse cached hashes where possible.
    FullCached,
}
