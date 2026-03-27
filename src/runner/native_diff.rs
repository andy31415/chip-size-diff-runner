use eyre::{Result, WrapErr, eyre};
use std::collections::HashMap;
use std::path::Path;
use std::fmt;
use elf::ElfBytes;
use elf::abi;
use elf::endian::AnyEndian;
use cpp_demangle;

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
enum SymbolType {
    NoType,
    Object,
    Func,
    Section,
    File,
    Common,
    Tls,
    Other(u8),
}

impl fmt::Display for SymbolType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SymbolType::NoType => write!(f, "NOTYPE"),
            SymbolType::Object => write!(f, "OBJECT"),
            SymbolType::Func => write!(f, "FUNC"),
            SymbolType::Section => write!(f, "SECTION"),
            SymbolType::File => write!(f, "FILE"),
            SymbolType::Common => write!(f, "COMMON"),
            SymbolType::Tls => write!(f, "TLS"),
            SymbolType::Other(o) => write!(f, "OTHER_{}", o),
        }
    }
}

impl From<u8> for SymbolType {
    fn from(stype: u8) -> Self {
        match stype {
            abi::STT_NOTYPE => SymbolType::NoType,
            abi::STT_OBJECT => SymbolType::Object,
            abi::STT_FUNC => SymbolType::Func,
            abi::STT_SECTION => SymbolType::Section,
            abi::STT_FILE => SymbolType::File,
            abi::STT_COMMON => SymbolType::Common,
            abi::STT_TLS => SymbolType::Tls,
            other => SymbolType::Other(other),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
struct Symbol {
    name: String,
    demangled: String,
    type_: SymbolType,
    size: u64,
}

fn demangle_name(name: &str) -> String {
    match cpp_demangle::Symbol::new(name.as_bytes()) {
        Ok(symbol) => symbol.to_string(),
        Err(_) => name.to_string(),
    }
}

fn parse_elf(file_path: &Path) -> Result<HashMap<String, Symbol>> {
    let path_str = file_path.to_str().ok_or_else(|| eyre!("Invalid path"))?;
    let file_data = std::fs::read(file_path)
        .wrap_err_with(|| format!("Failed to read ELF file: {}", path_str))?;
    let elf_file = ElfBytes::<AnyEndian>::minimal_parse(&file_data)
        .map_err(|e| eyre!("Failed to parse ELF file {}: {}", path_str, e))?;

    let mut symbols_map = HashMap::new();

    let (symtab, strtab) = elf_file.symbol_table()?        
        .ok_or_else(|| eyre!("No symbol table found in {}", path_str))?;

    for sym in symtab {
        let name: &str = strtab.get(sym.st_name as usize)?;
        if name.is_empty() || sym.st_size == 0 {
            continue;
        }

        let type_ = SymbolType::from(sym.st_symtype());
        let demangled = demangle_name(name);
        symbols_map.insert(
            name.to_string(),
            Symbol {
                name: name.to_string(),
                demangled,
                type_,
                size: sym.st_size,
            },
        );
    }

    Ok(symbols_map)
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum ChangeType {
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

struct DiffResult {
    change_type: ChangeType,
    symbol_type: SymbolType,
    symbol_name: String,
    diff: i64,
    base_size: u64,
    size: u64,
}

pub fn run_native_diff(from_path: &Path, to_path: &Path) -> Result<String> {
    let from_symbols = parse_elf(from_path)?;
    let to_symbols = parse_elf(to_path)?;

    let mut results: Vec<DiffResult> = Vec::new();

    let mut all_keys: Vec<&String> = from_symbols.keys().collect();
    for key in to_symbols.keys() {
        if !from_symbols.contains_key(key) {
            all_keys.push(key);
        }
    }
    all_keys.sort();
    all_keys.dedup();

    for key in all_keys {
        let from_sym = from_symbols.get(key);
        let to_sym = to_symbols.get(key);

        let size1 = from_sym.map(|s| s.size).unwrap_or(0);
        let size2 = to_sym.map(|s| s.size).unwrap_or(0);
        let diff = size2 as i64 - size1 as i64;

        if diff == 0 {
            continue;
        }

        let change_type = match (from_sym, to_sym) {
            (Some(_), Some(_)) => ChangeType::Changed,
            (None, Some(_)) => ChangeType::Added,
            (Some(_), None) => ChangeType::Removed,
            (None, None) => unreachable!(), // Should not happen due to key collection
        };

        let symbol = from_sym.or(to_sym).unwrap(); // Safe to unwrap here

        results.push(DiffResult {
            change_type,
            symbol_type: symbol.type_,
            symbol_name: symbol.demangled.clone(),
            diff,
            base_size: size1,
            size: size2,
        });
    }

    // Sort results by diff in descending order
    results.sort_by(|a, b| b.diff.cmp(&a.diff));

    let mut csv_output = String::new();
    csv_output.push_str("Change,Type,Symbol,Diff,Base Size,Size
");

    for result in results {
        let escaped_name = result.symbol_name.replace('"', r#"""#);
        let line = format!(
            r#"{},{},"{}",{},{},{}
"#,
            result.change_type,
            result.symbol_type,
            escaped_name,
            result.diff,
            result.base_size,
            result.size
        );
        csv_output.push_str(&line);
    }

    Ok(csv_output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_demangle_name() {
        assert_eq!(demangle_name("_ZN6System5Layer4InitEv"), "System::Layer::Init()");
        assert_eq!(demangle_name("not_mangled"), "not_mangled");
    }
}
