use eyre::Result;
use std::fmt;
use std::path::Path;

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum SymbolKind {
    Code,
    Data,
    Bss,
    RoData,
    Weak,
    Undefined,
    Other,
}

impl fmt::Display for SymbolKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SymbolKind::Code => write!(f, "CODE"),
            SymbolKind::Data => write!(f, "DATA"),
            SymbolKind::Bss => write!(f, "BSS"),
            SymbolKind::RoData => write!(f, "RODATA"),
            SymbolKind::Weak => write!(f, "WEAK"),
            SymbolKind::Undefined => write!(f, "UNDEF"),
            SymbolKind::Other => write!(f, "OTHER"),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct Symbol {
    pub name: String,
    pub demangled: String,
    pub kind: SymbolKind,
    pub size: usize,
    pub address: Option<u64>,
}

pub trait ElfParser {
    fn get_symbols(&self, path: &Path) -> Result<Vec<Symbol>>;
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
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

pub struct DiffResult {
    pub change_type: ChangeType,
    pub symbol_kind: SymbolKind,
    pub symbol_name: String,
    pub diff: i64,
    pub base_size: usize,
    pub size: usize,
}
