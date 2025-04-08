use std::sync::atomic::{AtomicUsize, Ordering};

/// The mode of id generator running
pub enum GeneratorMode {
    /// In normal mode generator should generate next unique id
    Normal,

    /// In dry run mode generator should return initial id to compare with input and output nodes ids
    DryRun,
}

/// Trait with id generator ability
pub trait IdGenerator<I> {
    fn generate_id(recent_id: &AtomicUsize, mode: GeneratorMode) -> I;
}

/// Default id generator implementation for usize id type
impl IdGenerator<usize> for () {
    fn generate_id(recent_id: &AtomicUsize, mode: GeneratorMode) -> usize {
        match mode {
            GeneratorMode::DryRun => recent_id.load(Ordering::Acquire),
            GeneratorMode::Normal => recent_id.fetch_add(1, Ordering::Release),
        }
    }
}

/// Default id generator implementation the for String id type
impl IdGenerator<String> for () {
    fn generate_id(recent_id: &AtomicUsize, mode: GeneratorMode) -> String {
        match mode {
            GeneratorMode::DryRun => {
                format!("ML_{}", recent_id.load(Ordering::Acquire))
            }
            GeneratorMode::Normal => {
                format!("ML_{}", recent_id.fetch_add(1, Ordering::Release))
            }
        }
    }
}