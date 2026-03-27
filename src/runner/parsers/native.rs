//! ELF symbol parser using the `elf` crate.

use crate::runner::elf_diff::{ElfParser, Symbol, SymbolKind};
use crate::runner::symbol_diff::demangle_name;
use elf::ElfBytes;
use elf::abi;
use elf::endian::AnyEndian;
use eyre::{Result, WrapErr, eyre};
use std::path::Path;

pub struct NativeParser;

impl ElfParser for NativeParser {
    fn get_symbols(&self, path: &Path) -> Result<Vec<Symbol>> {
        let path_str = path
            .to_str()
            .ok_or_else(|| eyre!("Invalid path: {:?}", path))?;
        let file_data = std::fs::read(path)
            .wrap_err_with(|| format!("Failed to read ELF file: {}", path_str))?;
        let elf_file = ElfBytes::<AnyEndian>::minimal_parse(&file_data)
            .map_err(|e| eyre!("Failed to parse ELF file {}: {}", path_str, e))?;

        let mut symbols = Vec::new();

        let (symtab, strtab) = elf_file
            .symbol_table()?
            .ok_or_else(|| eyre!("No symbol table found in {}", path_str))?;

        for sym in symtab {
            let name: &str = strtab.get(sym.st_name as usize)?;
            if name.is_empty() {
                continue;
            }

            let kind = match sym.st_symtype() {
                abi::STT_NOTYPE => SymbolKind::Other,
                abi::STT_OBJECT => SymbolKind::Data,
                abi::STT_FUNC => SymbolKind::Code,
                abi::STT_SECTION => SymbolKind::Other, // Changed from Section
                abi::STT_FILE => SymbolKind::Other,
                abi::STT_COMMON => SymbolKind::Bss,
                abi::STT_TLS => SymbolKind::Data,
                _ => SymbolKind::Other,
            };

            let demangled = demangle_name(name);
            symbols.push(Symbol {
                name: name.to_string(),
                demangled,
                kind,
                size: sym.st_size as usize,
                address: Some(sym.st_value),
            });
        }
        Ok(symbols)
    }
}
