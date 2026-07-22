//! Metadata-only inspectors. They extract *shape* — line counts, row counts — from
//! artifact bytes, but never place conversation content into the returned value. This is
//! the code path guarded by INV-3, which is output-side (ADR-017): reading bytes to derive
//! a count is fine; only the count leaves — content must never be *emitted*.

use std::io::Read;
use std::path::Path;

use serde::Serialize;

use crate::model::Definition;

/// Shape facts about an artifact. Every field is a count or size — never content.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Inspection {
    /// Number of newline-delimited records (≈ conversation turns) for JSONL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_count: Option<u64>,
    /// Total rows across the inspected tables for SQLite.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub row_count: Option<u64>,
}

impl Inspection {
    fn is_empty(&self) -> bool {
        self.line_count.is_none() && self.row_count.is_none()
    }
}

/// Run the definition's inspector against `path`, returning shape metadata or `None`
/// (no inspector, or the inspector degraded on an unreadable/corrupt file).
#[must_use]
pub fn inspect(sig: &Definition, path: &Path) -> Option<Inspection> {
    let result = match sig.inspector.as_deref() {
        Some("jsonl_linecount") => Inspection {
            line_count: count_lines(path),
            ..Default::default()
        },
        Some("sqlite_rowcount") => Inspection {
            row_count: count_rows(path, sig),
            ..Default::default()
        },
        _ => return None,
    };
    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

/// Count `\n` bytes by streaming — never materializes or inspects a line's content.
fn count_lines(path: &Path) -> Option<u64> {
    let file = std::fs::File::open(path).ok()?;
    let mut reader = std::io::BufReader::new(file);
    let mut buf = [0u8; 64 * 1024];
    let mut count = 0u64;
    loop {
        let n = reader.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        count += bytecount(&buf[..n], b'\n');
    }
    Some(count)
}

fn bytecount(haystack: &[u8], needle: u8) -> u64 {
    haystack.iter().filter(|&&b| b == needle).count() as u64
}

/// Open a SQLite database read-only, falling back to an immutable URI open for
/// locked / WAL databases that a plain read-only open cannot access.
fn open_readonly(path: &Path) -> Option<rusqlite::Connection> {
    use rusqlite::{Connection, OpenFlags};
    let ro = OpenFlags::SQLITE_OPEN_READ_ONLY;
    if let Ok(conn) = Connection::open_with_flags(path, ro) {
        return Some(conn);
    }
    let uri = format!("file:{}?immutable=1", uri_encode_path(path));
    Connection::open_with_flags(uri, ro | OpenFlags::SQLITE_OPEN_URI).ok()
}

/// Percent-encode a path for use in a SQLite `file:` URI (handles spaces, etc.).
fn uri_encode_path(path: &Path) -> String {
    let mut out = String::new();
    let mut buf = [0u8; 4];
    for ch in path.to_string_lossy().chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '/' | '-' | '.' | '_' | '~') {
            out.push(ch);
        } else {
            for b in ch.encode_utf8(&mut buf).as_bytes() {
                out.push('%');
                out.push_str(&format!("{b:02X}"));
            }
        }
    }
    out
}

/// Count rows via `SELECT count(*)` only — reads no content columns (INV-3). If the
/// definition names tables, use them; otherwise sum every user table. Any error
/// (locked/corrupt DB, missing table) degrades to `None`.
fn count_rows(path: &Path, sig: &Definition) -> Option<u64> {
    let conn = open_readonly(path)?;

    let tables = configured_tables(sig).unwrap_or_else(|| all_user_tables(&conn));
    if tables.is_empty() {
        return None;
    }

    let mut total = 0u64;
    let mut counted_any = false;
    for table in tables {
        // Table name is validated against the DB's own catalog before use, so it is
        // never attacker-influenced free text in the SQL.
        if !table_exists(&conn, &table) {
            continue;
        }
        let sql = format!("SELECT count(*) FROM \"{}\"", table.replace('"', "\"\""));
        if let Ok(n) = conn.query_row(&sql, [], |row| row.get::<_, i64>(0)) {
            total += u64::try_from(n).unwrap_or(0);
            counted_any = true;
        }
    }
    counted_any.then_some(total)
}

fn configured_tables(sig: &Definition) -> Option<Vec<String>> {
    let tables = sig.inspector_args.as_ref()?.get("tables")?.as_array()?;
    let names: Vec<String> = tables
        .iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();
    if names.is_empty() {
        None
    } else {
        Some(names)
    }
}

fn all_user_tables(conn: &rusqlite::Connection) -> Vec<String> {
    let mut stmt = match conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'")
    {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let rows = stmt.query_map([], |row| row.get::<_, String>(0));
    match rows {
        Ok(iter) => iter.filter_map(Result::ok).collect(),
        Err(_) => Vec::new(),
    }
}

fn table_exists(conn: &rusqlite::Connection, table: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1",
        [table],
        |_| Ok(()),
    )
    .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Definition;

    fn sig_with_inspector(inspector: &str, args: Option<serde_json::Value>) -> Definition {
        Definition {
            schema_version: 1,
            id: "t".into(),
            tool: "T".into(),
            vendor: None,
            platforms: vec![],
            paths: vec![],
            category: crate::model::Category::Transcript,
            format: crate::model::Format::Jsonl,
            sensitivity: crate::model::Sensitivity::High,
            inspector: Some(inspector.into()),
            inspector_args: args,
            retention_hint: None,
            why: "t".into(),
            guidance: vec![],
            confidence: None,
            references: vec![],
            ..Default::default()
        }
    }

    #[test]
    fn counts_jsonl_lines() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.jsonl");
        std::fs::write(&p, "one\ntwo\nthree\n").unwrap();
        let insp = inspect(&sig_with_inspector("jsonl_linecount", None), &p).unwrap();
        assert_eq!(insp.line_count, Some(3));
    }

    #[test]
    fn counts_sqlite_rows_for_configured_table() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("state.vscdb");
        let conn = rusqlite::Connection::open(&p).unwrap();
        conn.execute("CREATE TABLE chat (id INTEGER, body TEXT)", [])
            .unwrap();
        conn.execute("INSERT INTO chat VALUES (1, 'a'), (2, 'b')", [])
            .unwrap();
        drop(conn);

        let args = serde_json::json!({ "tables": ["chat"] });
        let insp = inspect(&sig_with_inspector("sqlite_rowcount", Some(args)), &p).unwrap();
        assert_eq!(insp.row_count, Some(2));
    }

    #[test]
    fn sqlite_sums_all_tables_when_unconfigured() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("db.sqlite");
        let conn = rusqlite::Connection::open(&p).unwrap();
        conn.execute("CREATE TABLE a (x INTEGER)", []).unwrap();
        conn.execute("CREATE TABLE b (y INTEGER)", []).unwrap();
        conn.execute("INSERT INTO a VALUES (1), (2)", []).unwrap();
        conn.execute("INSERT INTO b VALUES (3)", []).unwrap();
        drop(conn);
        let insp = inspect(&sig_with_inspector("sqlite_rowcount", None), &p).unwrap();
        assert_eq!(insp.row_count, Some(3));
    }

    #[test]
    fn counts_rows_in_a_wal_mode_database() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("wal.vscdb");
        let conn = rusqlite::Connection::open(&p).unwrap();
        conn.pragma_update(None, "journal_mode", "WAL").unwrap();
        conn.execute("CREATE TABLE t (x INTEGER)", []).unwrap();
        conn.execute("INSERT INTO t VALUES (1), (2), (3)", [])
            .unwrap();
        drop(conn);
        let insp = inspect(&sig_with_inspector("sqlite_rowcount", None), &p).unwrap();
        assert_eq!(insp.row_count, Some(3));
    }

    #[test]
    fn counts_a_large_jsonl_by_streaming() {
        // Exercises the streaming counter on a many-line file (constant memory).
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("big.jsonl");
        {
            use std::io::Write;
            let f = std::fs::File::create(&p).unwrap();
            let mut w = std::io::BufWriter::new(f);
            for _ in 0..500_000 {
                w.write_all(b"{\"m\":1}\n").unwrap();
            }
        }
        let insp = inspect(&sig_with_inspector("jsonl_linecount", None), &p).unwrap();
        assert_eq!(insp.line_count, Some(500_000));
    }

    #[test]
    fn corrupt_sqlite_degrades_to_none() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("bad.vscdb");
        std::fs::write(&p, b"this is not a sqlite file").unwrap();
        assert!(inspect(&sig_with_inspector("sqlite_rowcount", None), &p).is_none());
    }

    #[test]
    fn unknown_inspector_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("x");
        std::fs::write(&p, "x").unwrap();
        assert!(inspect(&sig_with_inspector("nope", None), &p).is_none());
    }

    #[test]
    fn fuzz_lite_inspectors_never_panic_on_arbitrary_bytes() {
        // Property: inspectors return an Option and never panic, whatever the bytes.
        let dir = tempfile::tempdir().unwrap();
        let jsig = sig_with_inspector("jsonl_linecount", None);
        let ssig = sig_with_inspector("sqlite_rowcount", None);
        for i in 0..256usize {
            let p = dir.path().join(format!("f{i}"));
            let len = (i * 7) % 400;
            let bytes: Vec<u8> = (0..len)
                .map(|j| ((j * 31 + i * 13 + 7) % 256) as u8)
                .collect();
            std::fs::write(&p, &bytes).unwrap();
            let _ = inspect(&jsig, &p);
            let _ = inspect(&ssig, &p);
        }
        // A truncated SQLite header followed by garbage must degrade, not panic.
        let p = dir.path().join("hdr");
        let mut v = b"SQLite format 3\0".to_vec();
        v.extend(std::iter::repeat(0xAB_u8).take(500));
        std::fs::write(&p, &v).unwrap();
        assert!(inspect(&ssig, &p).is_none());
    }

    #[test]
    fn uri_encode_handles_spaces_and_unicode() {
        let enc = uri_encode_path(std::path::Path::new("/a b/☕/c.db"));
        assert!(enc.starts_with("/a%20b/"));
        assert!(!enc.contains(' '));
    }
}
