use eyre::Result;
use std::fmt;
use std::path::Path;

/// Represents the type of a symbol (e.g., code, data, BSS).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SymbolKind {
    Code,    // Function or other executable code
    Data,    // Initialized data
    Bss,     // Uninitialized data
    Other,   // Other symbol types (e.g., section, file)
    Unknown, // Symbol type could not be determined
}

impl fmt::Display for SymbolKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SymbolKind::Code => write!(f, "Code"),
            SymbolKind::Data => write!(f, "Data"),
            SymbolKind::Bss => write!(f, "Bss"),
            SymbolKind::Other => write!(f, "Other"),
            SymbolKind::Unknown => write!(f, "Unknown"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Symbol {
    pub name: String,
    pub demangled: String,
    pub kind: SymbolKind,
    pub size: usize,
    pub address: Option<u64>,
}

/// Represents the change in size of a symbol between two ELF files.
#[derive(Debug, PartialEq)]
pub struct DiffResult {
    pub change_type: ChangeType,
    pub symbol_kind: SymbolKind,
    pub symbol_name: String,
    pub diff: i64,
    pub base_size: usize,
    pub size: usize,
}

/// Type of change for a symbol.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChangeType {
    Added,
    Removed,
    Changed,
}

impl fmt::Display for ChangeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChangeType::Added => write!(f, "ADDED"),
            ChangeType::Removed => write!(f, "REMOVED"),
            ChangeType::Changed => write!(f, "CHANGED"),
        }
    }
}

/// A trait for parsing ELF files to extract symbol information.
/// This provides a common interface for different ELF parsing implementations.
pub trait ElfParser {
    fn get_symbols(&self, path: &Path) -> Result<Vec<Symbol>>;
}
