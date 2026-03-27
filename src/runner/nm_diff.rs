use eyre::{Result, WrapErr, eyre};
use std::collections::HashMap;
use std::process::Command;
use std::path::Path;

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
struct Symbol {
    name: String,
    type_: char,
    size: u64,
}

fn parse_nm_output(output: &str) -> Result<HashMap<String, Symbol>> {
    let mut symbols = HashMap::new();
    for line in output.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 {
            continue;
        }

        let size_str = parts[0];
        let type_ = parts[1].chars().next().ok_or_else(|| eyre!("Invalid type in nm output"))?;
        let name = parts[2].to_string();

        if size_str == "" || name == "" {
            continue;
        }

        let size = u64::from_str_radix(size_str, 16)
            .wrap_err_with(|| format!("Failed to parse size from nm output: {}", size_str))?;

        symbols.insert(name.clone(), Symbol { name, type_, size });
    }
    Ok(symbols)
}

fn run_nm(file_path: &Path) -> Result<String> {
    let output = Command::new("nm")
        .arg("-S")
        .arg("--size-sort")
        .arg(file_path)
        .output()
        .wrap_err_with(|| format!("Failed to run nm on {}", file_path.display()))?;

    if !output.status.success() {
        return Err(eyre!(
            "nm failed for {}: {}
{}",
            file_path.display(),
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    String::from_utf8(output.stdout)
        .wrap_err_with(|| format!("Failed to parse nm output for {}", file_path.display()))
}

pub fn run_nm_diff(from_path: &Path, to_path: &Path) -> Result<String> {
    let from_output = run_nm(from_path)?;
    let to_output = run_nm(to_path)?;

    let from_symbols = parse_nm_output(&from_output)?;
    let to_symbols = parse_nm_output(&to_output)?;

    let mut csv_output = String::new();
    csv_output.push_str("Function,Type,Size1,Size2,Diff
");

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

        let name = key;
        let type_ = from_sym.map(|s| s.type_).or_else(|| to_sym.map(|s| s.type_)).unwrap_or('?');
        let size1 = from_sym.map(|s| s.size).unwrap_or(0);
        let size2 = to_sym.map(|s| s.size).unwrap_or(0);
        let diff = size2 as i64 - size1 as i64;

        csv_output.push_str(&format!("{},{},{},{},{}
", name, type_, size1, size2, diff));
    }

    Ok(csv_output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_nm_output_valid() {
        let output = "0000000000000010 T func1
0000000000000020 D data1
                 U undefined
";
        let symbols = parse_nm_output(output).unwrap();
        assert_eq!(symbols.len(), 2);
        assert_eq!(symbols.get("func1"), Some(&Symbol { name: "func1".to_string(), type_: 'T', size: 16 }));
        assert_eq!(symbols.get("data1"), Some(&Symbol { name: "data1".to_string(), type_: 'D', size: 32 }));
    }

    #[test]
    fn test_parse_nm_output_empty() {
        let output = "";
        let symbols = parse_nm_output(output).unwrap();
        assert_eq!(symbols.len(), 0);
    }
}
