//! ELF symbol parser using the `goblin` crate.

use crate::runner::elf_diff::{ElfParser, Symbol, SymbolKind};
use crate::runner::symbol_diff::demangle_name;
use eyre::{Result, WrapErr, eyre};
use goblin::elf;
use goblin::elf::sym;
use std::fs;
use std::path::Path;

pub struct GoblinParser;

impl ElfParser for GoblinParser {
    fn get_symbols(&self, path: &Path) -> Result<Vec<Symbol>> {
        let path_str = path
            .to_str()
            .ok_or_else(|| eyre!("Invalid path: {:?}", path))?;
        let buffer =
            fs::read(path).wrap_err_with(|| format!("Failed to read ELF file: {}", path_str))?;

        let elf = elf::Elf::parse(&buffer)
            .map_err(|e| eyre!("Failed to parse ELF file {}: {}", path_str, e))?;

        let mut symbols = Vec::new();

        for sym in elf.syms.iter().chain(elf.dynsyms.iter()) {
            if let Some(name) = elf.strtab.get_at(sym.st_name) {
                if name.is_empty() {
                    continue;
                }

                let kind = match sym.st_type() {
                    sym::STT_NOTYPE => SymbolKind::Other,
                    sym::STT_OBJECT => SymbolKind::Data,
                    sym::STT_FUNC => SymbolKind::Code,
                    sym::STT_SECTION => SymbolKind::Other,
                    sym::STT_FILE => SymbolKind::Other,
                    sym::STT_COMMON => SymbolKind::Bss,
                    sym::STT_TLS => SymbolKind::Data,
                    _ => SymbolKind::Other,
                };

                // In Goblin, BSS symbols often have size 0 in the symbol table,
                // but they do occupy space. We'll rely on the section size for BSS.
                // For now, let's just use st_size.
                let size = sym.st_size as usize;

                // Skip symbols with no size, unless they are functions (which can sometimes be size 0).
                if size == 0 && kind != SymbolKind::Code {
                    // Further check if it's BSS by looking at the section index
                    if sym.st_shndx == elf::section_header::SHN_COMMON as usize {
                        // This is a common block (BSS), keep it, size will be from section
                    } else if kind != SymbolKind::Bss { // Non-BSS, non-Code, zero size can be skipped
                        // continue; // Re-evaluate if skipping is always correct
                    }
                }

                let demangled = demangle_name(name);
                symbols.push(Symbol {
                    name: name.to_string(),
                    demangled,
                    kind,
                    size,
                    address: Some(sym.st_value),
                });
            }
        }
        Ok(symbols)
    }
}
