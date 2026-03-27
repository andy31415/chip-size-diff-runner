use crate::runner::definitions::{ElfParser, Symbol, SymbolKind};
use crate::runner::common::demangle_name;
use eyre::{Result, WrapErr, eyre};
use std::path::Path;
use std::process::Command;

pub struct NmParser {
    pub nm_path: String,
}

impl Default for NmParser {
    fn default() -> Self {
        NmParser {
            nm_path: "nm".to_string(),
        }
    }
}

impl ElfParser for NmParser {
    fn get_symbols(&self, path: &Path) -> Result<Vec<Symbol>> {
        tracing::debug!("Getting symbol sizes for file (nm): {:?}", path);
        let output = Command::new(&self.nm_path)
            .arg("--print-size")
            // .arg("--size-sort") // Sorting is done in common diff
            .arg("--radix=d") // Decimal radix for size and address
            .arg(path)
            .output()
            .wrap_err("Failed to execute nm")?;

        if !output.status.success() {
            return Err(eyre!(
                "nm failed with exit code {}: {}
stderr: {}",
                output.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        let output_str = String::from_utf8_lossy(&output.stdout);
        let mut symbols = Vec::new();

        for line in output_str.lines() {
            if line.trim().is_empty() {
                continue;
            }

            let parts: Vec<&str> = line.splitn(4, ' ').collect();
            if parts.len() != 4 {
                tracing::warn!("Skipping malformed nm line: {}", line);
                continue;
            }

            let address: u64 = parts[0]
                .parse()
                .wrap_err_with(|| format!("Failed to parse address from nm line: {}", line))?;
            let size: usize = parts[1]
                .parse()
                .wrap_err_with(|| format!("Failed to parse size from nm line: {}", line))?;
            
            if size == 0 {
                continue; // Skip zero-sized symbols
            }
            
            let symbol_type = parts[2].chars().next().unwrap_or('?');
            let name = parts[3];

            let kind = match symbol_type {
                'T' | 't' => SymbolKind::Code,
                'D' | 'd' => SymbolKind::Data,
                'B' | 'b' => SymbolKind::Bss,
                'R' | 'r' => SymbolKind::RoData,
                'W' | 'w' => SymbolKind::Weak,
                'U' => SymbolKind::Undefined,
                _ => SymbolKind::Other,
            };

            let demangled = demangle_name(name);

            symbols.push(Symbol {
                name: name.to_string(),
                demangled,
                kind,
                size,
                address: Some(address),
            });
        }

        tracing::debug!("Found {} symbols in {:?} (nm)", symbols.len(), path);
        Ok(symbols)
    }
}
