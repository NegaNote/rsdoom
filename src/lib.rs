pub mod argparse;
pub mod wad;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineMode {
    Doom1,
    Doom2,
}
