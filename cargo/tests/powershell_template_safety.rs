use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

const ALLOWED_SCOPES: &[&str] = &[
    "env", "global", "script", "local", "private", "function", "variable", "using", "workflow",
];

#[derive(Debug, Clone)]
struct RawStringBlock {
    content: String,
    start_line: usize,
}

fn is_ident_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

fn is_ident_continue(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            collect_rs_files(&path, out)?;
            continue;
        }

        if path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("rs"))
        {
            out.push(path);
        }
    }

    Ok(())
}

fn invalid_var_colon_tokens(line: &str, allowed: &HashSet<&'static str>) -> Vec<String> {
    let chars: Vec<char> = line.chars().collect();
    let mut invalid = Vec::new();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] != '$' {
            i += 1;
            continue;
        }

        let start = i + 1;
        if start >= chars.len() || !is_ident_start(chars[start]) {
            i += 1;
            continue;
        }

        let mut end = start + 1;
        while end < chars.len() && is_ident_continue(chars[end]) {
            end += 1;
        }

        if end < chars.len() && chars[end] == ':' {
            let ident: String = chars[start..end].iter().collect();
            if !allowed.contains(ident.to_ascii_lowercase().as_str()) {
                invalid.push(format!("${ident}:"));
            }
        }

        i = end;
    }

    invalid
}

fn line_number_for_byte(content: &str, byte_idx: usize) -> usize {
    content[..byte_idx].bytes().filter(|b| *b == b'\n').count() + 1
}

fn extract_raw_string_blocks(content: &str) -> Vec<RawStringBlock> {
    let bytes = content.as_bytes();
    let mut blocks = Vec::new();
    let mut idx = 0;

    while idx < bytes.len() {
        if bytes[idx] != b'r' {
            idx += 1;
            continue;
        }

        let mut hash_end = idx + 1;
        while hash_end < bytes.len() && bytes[hash_end] == b'#' {
            hash_end += 1;
        }

        let hash_count = hash_end.saturating_sub(idx + 1);
        if hash_count == 0 || hash_end >= bytes.len() || bytes[hash_end] != b'"' {
            idx += 1;
            continue;
        }

        let content_start = hash_end + 1;
        let mut cursor = content_start;
        let mut content_end = None;

        while cursor < bytes.len() {
            if bytes[cursor] == b'"'
                && cursor + hash_count < bytes.len()
                && (0..hash_count).all(|offset| bytes[cursor + 1 + offset] == b'#')
            {
                content_end = Some(cursor);
                break;
            }

            cursor += 1;
        }

        if let Some(end_idx) = content_end {
            blocks.push(RawStringBlock {
                content: content[content_start..end_idx].to_string(),
                start_line: line_number_for_byte(content, content_start),
            });
            idx = end_idx + 1 + hash_count;
            continue;
        }

        idx += 1;
    }

    blocks
}

fn first_top_level_executable_line(script: &str) -> Option<(usize, String)> {
    let mut brace_depth: i32 = 0;

    for (line_idx, line) in script.lines().enumerate() {
        let trimmed = line.trim();
        if brace_depth == 0
            && !trimmed.is_empty()
            && !trimmed.starts_with('#')
            && !trimmed.starts_with("<#")
            && !trimmed.starts_with("#>")
        {
            return Some((line_idx + 1, trimmed.to_string()));
        }

        brace_depth += line.chars().filter(|c| *c == '{').count() as i32;
        brace_depth -= line.chars().filter(|c| *c == '}').count() as i32;
        if brace_depth < 0 {
            brace_depth = 0;
        }
    }

    None
}

fn first_top_level_param_line(script: &str) -> Option<(usize, String)> {
    let mut brace_depth: i32 = 0;

    for (line_idx, line) in script.lines().enumerate() {
        let trimmed = line.trim();
        if brace_depth == 0 && trimmed.starts_with("param(") {
            return Some((line_idx + 1, trimmed.to_string()));
        }

        brace_depth += line.chars().filter(|c| *c == '{').count() as i32;
        brace_depth -= line.chars().filter(|c| *c == '}').count() as i32;
        if brace_depth < 0 {
            brace_depth = 0;
        }
    }

    None
}

#[test]
fn powershell_templates_do_not_use_invalid_variable_drive_syntax() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let src_dir = manifest_dir.join("src");
    let allowed: HashSet<&'static str> = ALLOWED_SCOPES.iter().copied().collect();

    let mut source_files = Vec::new();
    collect_rs_files(&src_dir, &mut source_files).expect("failed to list source files");

    let mut violations = Vec::new();
    for file_path in source_files {
        let content = fs::read_to_string(&file_path)
            .unwrap_or_else(|e| panic!("failed to read {}: {}", file_path.display(), e));

        for (line_idx, line) in content.lines().enumerate() {
            for token in invalid_var_colon_tokens(line, &allowed) {
                let rel_path = file_path
                    .strip_prefix(&manifest_dir)
                    .unwrap_or(&file_path)
                    .display()
                    .to_string();
                violations.push(format!(
                    "{}:{} contains {} in: {}",
                    rel_path,
                    line_idx + 1,
                    token,
                    line.trim()
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Found potential invalid PowerShell variable-drive interpolations:\n{}",
        violations.join("\n")
    );
}

#[test]
fn powershell_templates_with_top_level_param_declare_it_first() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let src_dir = manifest_dir.join("src");

    let mut source_files = Vec::new();
    collect_rs_files(&src_dir, &mut source_files).expect("failed to list source files");

    let mut violations = Vec::new();
    for file_path in source_files {
        let content = fs::read_to_string(&file_path)
            .unwrap_or_else(|e| panic!("failed to read {}: {}", file_path.display(), e));
        let raw_blocks = extract_raw_string_blocks(&content);

        for block in raw_blocks {
            let Some((param_rel_line, _)) = first_top_level_param_line(&block.content) else {
                continue;
            };
            let Some((exec_rel_line, exec_line)) = first_top_level_executable_line(&block.content)
            else {
                continue;
            };

            if exec_rel_line != param_rel_line {
                let rel_path = file_path
                    .strip_prefix(&manifest_dir)
                    .unwrap_or(&file_path)
                    .display()
                    .to_string();
                let exec_file_line = block.start_line + exec_rel_line - 1;
                let param_file_line = block.start_line + param_rel_line - 1;

                violations.push(format!(
                    "{}:{} first executable statement is '{}' but top-level param() starts at {}",
                    rel_path, exec_file_line, exec_line, param_file_line
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Found PowerShell template param-order violations:\n{}",
        violations.join("\n")
    );
}
