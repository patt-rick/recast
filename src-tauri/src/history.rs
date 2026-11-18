//! Job history in SQLite at %LOCALAPPDATA%\Recast\recast.db (spec §6.5).
use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

const SCHEMA_VERSION: &str = "1";
const COLS: &str = "id, preset_id, inputs, output, status, error_code, error_detail, stderr_tail, started_at, ended_at, temp_path";
const UNFINISHED: &str = "('queued','waiting_engine','running')";

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct JobRecord {
    pub id: String,
    pub preset_id: String,
    pub inputs: Vec<String>,
    pub output: Option<String>,
    pub status: String,
    pub error_code: Option<String>,
    pub error_detail: Option<String>,
    pub stderr_tail: Option<String>,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub temp_path: Option<String>,
}

#[derive(Default)]
pub struct Finish<'a> {
    pub status: &'a str,
    pub output: Option<&'a str>,
    pub error_code: Option<&'a str>,
    pub error_detail: Option<&'a str>,
    pub stderr_tail: Option<&'a str>,
}

pub struct History {
    conn: Connection,
}

fn row(r: &rusqlite::Row) -> rusqlite::Result<JobRecord> {
    let inputs: String = r.get("inputs")?;
    Ok(JobRecord {
        id: r.get("id")?,
        preset_id: r.get("preset_id")?,
        inputs: serde_json::from_str(&inputs).unwrap_or_default(),
        output: r.get("output")?,
        status: r.get("status")?,
        error_code: r.get("error_code")?,
        error_detail: r.get("error_detail")?,
        stderr_tail: r.get("stderr_tail")?,
        started_at: r.get("started_at")?,
        ended_at: r.get("ended_at")?,
        temp_path: r.get("temp_path")?,
    })
}

impl History {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        if let Some(d) = path.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let h = Self { conn: Connection::open(path)? };
        h.migrate()?;
        Ok(h)
    }

    pub fn open_in_memory() -> rusqlite::Result<Self> {
        let h = Self { conn: Connection::open_in_memory()? };
        h.migrate()?;
        Ok(h)
    }

    fn migrate(&self) -> rusqlite::Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS jobs (
               seq INTEGER PRIMARY KEY AUTOINCREMENT,
               id TEXT NOT NULL UNIQUE,
               preset_id TEXT NOT NULL,
               inputs TEXT NOT NULL,
               output TEXT,
               status TEXT NOT NULL,
               error_code TEXT,
               error_detail TEXT,
               stderr_tail TEXT,
               started_at INTEGER NOT NULL,
               ended_at INTEGER,
               temp_path TEXT);",
        )?;
        self.conn.execute("INSERT OR IGNORE INTO meta (key, value) VALUES ('schema_version', ?1)", params![SCHEMA_VERSION])?;
        Ok(())
    }

    pub fn insert(&self, id: &str, preset_id: &str, inputs: &[String], now: i64) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO jobs (id, preset_id, inputs, status, started_at) VALUES (?1, ?2, ?3, 'queued', ?4)",
            params![id, preset_id, serde_json::to_string(inputs).unwrap(), now],
        )?;
        Ok(())
    }

    pub fn set_status(&self, id: &str, status: &str) -> rusqlite::Result<()> {
        self.conn.execute("UPDATE jobs SET status = ?2 WHERE id = ?1", params![id, status])?;
        Ok(())
    }

    pub fn set_temp(&self, id: &str, temp: Option<&str>) -> rusqlite::Result<()> {
        self.conn.execute("UPDATE jobs SET temp_path = ?2 WHERE id = ?1", params![id, temp])?;
        Ok(())
    }

    pub fn finish(&self, id: &str, f: &Finish, now: i64) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE jobs SET status = ?2, output = ?3, error_code = ?4, error_detail = ?5, stderr_tail = ?6,
                             ended_at = ?7 WHERE id = ?1",
            params![id, f.status, f.output, f.error_code, f.error_detail, f.stderr_tail, now],
        )?;
        Ok(())
    }

    pub fn get(&self, id: &str) -> rusqlite::Result<Option<JobRecord>> {
        self.conn.query_row(&format!("SELECT {COLS} FROM jobs WHERE id = ?1"), params![id], row).optional()
    }

    pub fn list(&self, page: u32, per_page: u32) -> rusqlite::Result<Vec<JobRecord>> {
        let mut st = self.conn.prepare(&format!("SELECT {COLS} FROM jobs ORDER BY seq DESC LIMIT ?1 OFFSET ?2"))?;
        let rows = st.query_map(params![per_page, page * per_page], row)?;
        rows.collect()
    }

    pub fn remove(&self, id: &str) -> rusqlite::Result<()> {
        self.conn.execute("DELETE FROM jobs WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Keep the newest `keep` rows; never delete unfinished jobs.
    pub fn prune(&self, keep: u32) -> rusqlite::Result<()> {
        self.conn.execute(
            &format!(
                "DELETE FROM jobs WHERE status NOT IN {UNFINISHED} AND temp_path IS NULL
                 AND seq NOT IN (SELECT seq FROM jobs ORDER BY seq DESC LIMIT ?1)"
            ),
            params![keep],
        )?;
        Ok(())
    }

    /// At startup: unfinished jobs become INTERRUPTED. Returns every temp dir still on
    /// record (theirs, and finished jobs' whose removal failed) to delete; call
    /// `forget_temp` for each one once it is gone, so a failed delete is retried.
    pub fn recover_interrupted(&self, now: i64) -> rusqlite::Result<Vec<String>> {
        let temps: Vec<String> = {
            let mut st = self.conn.prepare("SELECT temp_path FROM jobs WHERE temp_path IS NOT NULL ORDER BY seq")?;
            let rows = st.query_map([], |r| r.get(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        self.conn.execute(
            &format!(
                "UPDATE jobs SET status = 'interrupted', error_code = 'INTERRUPTED',
                 error_detail = 'Recast closed before this finished.', ended_at = ?1
                 WHERE status IN {UNFINISHED}"
            ),
            params![now],
        )?;
        Ok(temps)
    }

    /// The temp dir is gone: stop tracking it.
    pub fn forget_temp(&self, temp: &str) -> rusqlite::Result<()> {
        self.conn.execute("UPDATE jobs SET temp_path = NULL WHERE temp_path = ?1", params![temp])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h() -> History {
        History::open_in_memory().unwrap()
    }

    #[test]
    fn insert_finish_get_list() {
        let h = h();
        h.insert("a", "video-mp3", &["C:\\x\\a.mov".into()], 100).unwrap();
        h.insert("b", "video-mp4", &["C:\\x\\b.mov".into()], 101).unwrap();
        h.finish("a", &Finish { status: "done", output: Some("C:\\x\\a.mp3"), ..Default::default() }, 110).unwrap();
        let a = h.get("a").unwrap().unwrap();
        assert_eq!(a.status, "done");
        assert_eq!(a.output.as_deref(), Some("C:\\x\\a.mp3"));
        assert_eq!(a.inputs, vec!["C:\\x\\a.mov".to_string()]);
        assert_eq!(a.ended_at, Some(110));
        let list = h.list(0, 50).unwrap();
        assert_eq!(list.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["b", "a"], "newest first");
    }

    #[test]
    fn recovery_marks_unfinished_jobs_interrupted_and_returns_temp_dirs() {
        let h = h();
        h.insert("a", "p", &[], 1).unwrap();
        h.set_status("a", "running").unwrap();
        h.set_temp("a", Some("C:\\x\\.recast-a")).unwrap();
        h.insert("b", "p", &[], 2).unwrap();
        h.finish("b", &Finish { status: "done", ..Default::default() }, 3).unwrap();
        // a finished job whose temp dir couldn't be removed
        h.insert("c", "p", &[], 4).unwrap();
        h.set_temp("c", Some(r"C:\x\.recast-c")).unwrap();
        h.finish("c", &Finish { status: "done", ..Default::default() }, 5).unwrap();
        assert_eq!(h.recover_interrupted(9).unwrap(), vec![r"C:\x\.recast-a".to_string(), r"C:\x\.recast-c".to_string()]);
        assert_eq!(h.get("c").unwrap().unwrap().status, "done");
        let a = h.get("a").unwrap().unwrap();
        assert_eq!(a.status, "interrupted");
        assert_eq!(a.error_code.as_deref(), Some("INTERRUPTED"));
        // still tracked until the caller has actually deleted it
        assert_eq!(a.temp_path.as_deref(), Some(r"C:\x\.recast-a"));
        h.forget_temp(r"C:\x\.recast-a").unwrap();
        assert_eq!(h.get("a").unwrap().unwrap().temp_path, None);
        assert_eq!(h.get("c").unwrap().unwrap().temp_path.as_deref(), Some(r"C:\x\.recast-c"));
        assert_eq!(h.get("b").unwrap().unwrap().status, "done");
    }

    #[test]
    fn prune_keeps_newest_finished_and_all_unfinished() {
        let h = h();
        for i in 0..10 {
            let id = format!("j{i}");
            h.insert(&id, "p", &[], i).unwrap();
            if i != 0 {
                h.finish(&id, &Finish { status: "done", ..Default::default() }, i).unwrap();
            }
        }
        h.set_temp("j1", Some(r"C:\x\.recast-j1")).unwrap();
        h.prune(3).unwrap();
        let ids: Vec<String> = h.list(0, 50).unwrap().into_iter().map(|r| r.id).collect();
        assert_eq!(ids, ["j9", "j8", "j7", "j1", "j0"], "a row whose temp dir is still on disk is kept");
    }

    #[test]
    fn remove_deletes_only_the_row() {
        let h = h();
        h.insert("a", "p", &[], 1).unwrap();
        h.remove("a").unwrap();
        assert!(h.get("a").unwrap().is_none());
    }

    #[test]
    fn reopening_a_file_keeps_rows() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("recast.db");
        History::open(&p).unwrap().insert("a", "p", &[], 1).unwrap();
        assert!(History::open(&p).unwrap().get("a").unwrap().is_some());
    }
}
