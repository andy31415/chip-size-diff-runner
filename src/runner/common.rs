use crate::runner::definitions::{Symbol, DiffResult, ChangeType};
use eyre::Result;
use std::collections::HashMap;
use cpp_demangle;

pub fn demangle_name(name: &str) -> String {
    match cpp_demangle::Symbol::new(name.as_bytes()) {
        Ok(symbol) => symbol.to_string(),
        Err(_) => name.to_string(),
    }
}

pub fn generate_diff_csv(from_symbols: Vec<Symbol>, to_symbols: Vec<Symbol>) -> Result<String> {
    let from_map: HashMap<String, Symbol> = from_symbols.into_iter().map(|s| (s.name.clone(), s)).collect();
    let to_map: HashMap<String, Symbol> = to_symbols.into_iter().map(|s| (s.name.clone(), s)).collect();

    let mut results: Vec<DiffResult> = Vec::new();
    let mut all_keys: Vec<&String> = from_map.keys().collect();
    for key in to_map.keys() {
        if !from_map.contains_key(key) {
            all_keys.push(key);
        }
    }
    all_keys.sort();
    all_keys.dedup();

    for key in all_keys {
        let from_sym = from_map.get(key);
        let to_sym = to_map.get(key);

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
            (None, None) => unreachable!(),
        };

        let symbol = from_sym.or(to_sym).unwrap();

        results.push(DiffResult {
            change_type,
            symbol_kind: symbol.kind,
            symbol_name: symbol.demangled.clone(),
            diff,
            base_size: size1,
            size: size2,
        });
    }

    results.sort_by(|a, b| b.diff.cmp(&a.diff));

    let mut csv_output = String::new();
    csv_output.push_str("Change,Type,Symbol,Diff,Base Size,Size
");

    for result in results {
        let escaped_name = result.symbol_name.replace('"', r#"""#);
        csv_output.push_str(&format!(
            r#"{},{},"{}",{},{},{}
"#,
            result.change_type,
            result.symbol_kind,
            escaped_name,
            result.diff,
            result.base_size,
            result.size
        ));
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
    // TODO: Add tests for generate_diff_csv
}
