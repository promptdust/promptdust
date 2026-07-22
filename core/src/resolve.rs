//! Turning a definition path pattern into concrete filesystem locations:
//! expand `~`/`$ENV`, split the literal base directory from the glob remainder, and
//! build a matcher. All read-only; this module never touches the filesystem except
//! callers walking the base it returns.

use std::path::{Path, PathBuf};

/// Characters that make a path component a glob rather than a literal.
const GLOB_META: &[char] = &['*', '?', '[', ']', '{', '}'];

/// Does the pattern contain any glob metacharacters?
#[must_use]
pub fn has_glob(pattern: &str) -> bool {
    pattern.contains(GLOB_META)
}

/// Expand a leading `~` and `$VAR` / `${VAR}` references against `home` and the
/// environment. Unknown variables expand to empty (leaving a harmless dead path that
/// simply won't match anything).
#[must_use]
pub fn expand(pattern: &str, home: &Path) -> String {
    let mut s = String::with_capacity(pattern.len() + 16);

    // Leading `~` → home.
    let rest = if pattern == "~" {
        s.push_str(&home.to_string_lossy());
        ""
    } else if let Some(stripped) = pattern.strip_prefix("~/") {
        s.push_str(&home.to_string_lossy());
        s.push('/');
        stripped
    } else {
        pattern
    };

    expand_env_into(rest, &mut s);
    s
}

fn expand_env_into(input: &str, out: &mut String) {
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' {
            // ${VAR}
            if i + 1 < bytes.len() && bytes[i + 1] == b'{' {
                if let Some(end) = input[i + 2..].find('}') {
                    let name = &input[i + 2..i + 2 + end];
                    push_var(name, out);
                    i = i + 2 + end + 1;
                    continue;
                }
            }
            // $VAR (alnum / underscore)
            let start = i + 1;
            let mut j = start;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            if j > start {
                push_var(&input[start..j], out);
                i = j;
                continue;
            }
        }
        // Not a variable: copy this byte through (input is valid UTF-8, ASCII step ok
        // because we only special-case ASCII '$').
        let ch_len = utf8_len(bytes[i]);
        out.push_str(&input[i..i + ch_len]);
        i += ch_len;
    }
}

fn push_var(name: &str, out: &mut String) {
    if let Ok(val) = std::env::var(name) {
        out.push_str(&val);
    }
}

const fn utf8_len(first: u8) -> usize {
    if first < 0x80 {
        1
    } else if first >> 5 == 0b110 {
        2
    } else if first >> 4 == 0b1110 {
        3
    } else {
        4
    }
}

/// Split an expanded, absolute pattern into the longest literal base directory and
/// the full pattern. Callers walk the base and match entries against the pattern.
///
/// e.g. `/Users/x/.claude/projects/**/*.jsonl` → base `/Users/x/.claude/projects`.
#[must_use]
pub fn split_base(expanded: &str) -> PathBuf {
    // The base is the longest literal prefix directory: everything up to the
    // separator before the first component that contains a glob metacharacter. We
    // slice the original string rather than re-`push`ing split components, because
    // rebuilding a Windows path component-by-component drops the drive root
    // (`C:\Users` becomes the drive-relative `C:Users`), which then never resolves.
    let Some(glob_idx) = expanded.find(|c: char| GLOB_META.contains(&c)) else {
        // No glob: the whole (literal) path is the base.
        return PathBuf::from(expanded);
    };
    match expanded[..glob_idx].rfind(['/', '\\']) {
        // Keep the leading separator when it is the filesystem root (`/foo*` → `/`).
        Some(0) => PathBuf::from(&expanded[..1]),
        Some(sep) => PathBuf::from(&expanded[..sep]),
        // A glob in the very first component (e.g. `*.jsonl`): no literal base dir.
        None => PathBuf::new(),
    }
}

/// Normalize a path to a forward-slash string for glob matching across platforms.
#[must_use]
pub fn to_match_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_tilde() {
        let home = Path::new("/Users/test");
        assert_eq!(expand("~/.claude/x", home), "/Users/test/.claude/x");
        assert_eq!(expand("~", home), "/Users/test");
        assert_eq!(expand("/abs/path", home), "/abs/path");
    }

    #[test]
    fn expands_env_vars() {
        std::env::set_var("PROMPTDUST_TEST_VAR", "VALUE");
        let home = Path::new("/h");
        assert_eq!(expand("~/$PROMPTDUST_TEST_VAR/x", home), "/h/VALUE/x");
        assert_eq!(expand("~/${PROMPTDUST_TEST_VAR}/x", home), "/h/VALUE/x");
        std::env::remove_var("PROMPTDUST_TEST_VAR");
    }

    #[test]
    fn unknown_env_var_expands_empty() {
        let home = Path::new("/h");
        assert_eq!(expand("~/$PROMPTDUST_NOPE_XYZ/x", home), "/h//x");
    }

    #[test]
    fn detects_globs() {
        assert!(has_glob("~/a/**/*.jsonl"));
        assert!(has_glob("~/a/file?.txt"));
        assert!(!has_glob("~/.claude.json"));
    }

    #[test]
    fn splits_base_at_first_glob() {
        assert_eq!(
            split_base("/Users/x/.claude/projects/**/*.jsonl"),
            PathBuf::from("/Users/x/.claude/projects")
        );
        assert_eq!(
            split_base("/Users/x/.claude.json"),
            PathBuf::from("/Users/x/.claude.json")
        );
    }

    #[test]
    fn splits_windows_drive_path_without_losing_root() {
        // Regression: a Windows-style expanded pattern must keep its `C:\` drive root
        // so the walk base resolves. Pure string logic, so this holds on every OS.
        assert_eq!(
            split_base(r"C:\Users\x\.claude\projects\**\*.jsonl").to_string_lossy(),
            r"C:\Users\x\.claude\projects"
        );
        // Mixed separators — what expand() produces on Windows (backslash HOME +
        // forward-slash pattern tail) — must also keep the drive root.
        assert_eq!(
            split_base(r"C:\Users\x/.claude/projects/**/*.jsonl").to_string_lossy(),
            r"C:\Users\x/.claude/projects"
        );
    }

    #[test]
    fn preserves_unicode_in_expand() {
        let home = Path::new("/h");
        assert_eq!(expand("~/café/日本/x", home), "/h/café/日本/x");
    }

    #[test]
    fn fuzz_lite_resolution_never_panics() {
        // Property: for any input, expand/split_base/has_glob must not panic. This is
        // the fuzzing intent on a stable toolchain (cargo-fuzz needs nightly).
        let home = Path::new("/home/user");
        let tokens = [
            "~", "/", "\\", "$", "${", "}", "*", "**", "?", "[", "]", "{", ",", "A", "é", "日",
            "$HOME", "${X}", "..", ".", " ", "",
        ];
        for &a in &tokens {
            for &b in &tokens {
                for &c in &tokens {
                    let s = format!("{a}{b}{c}");
                    let e = expand(&s, home);
                    let _ = split_base(&e);
                    let _ = has_glob(&s);
                    let _ = to_match_string(Path::new(&e));
                }
            }
        }
        // Pathologically long / deep inputs.
        let long = format!("~/{}**/*.jsonl", "a/".repeat(3000));
        let e = expand(&long, home);
        let _ = split_base(&e);
    }
}
