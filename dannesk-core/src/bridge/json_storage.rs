//src/utils/json_storage.rs
//!
//! ## Why every write goes through a temporary file
//!
//! These files hold the only copy of a wallet's key material. A plain
//! `fs::write` truncates the destination and *then* streams into it, so a crash,
//! a full disk or a power cut between those two moments leaves a real file
//! containing half a ciphertext — and the seed it used to hold is gone. There is
//! no second copy to fall back to: the app is not the backup.
//!
//! So the destination is never opened for writing at all. The bytes go to a
//! sibling `.tmp`, get flushed to the platter, and only then does `rename`
//! swap them in — an operation POSIX guarantees is atomic. Any reader either
//! sees the whole old file or the whole new one, never a partial write, and any
//! failure before the rename leaves the original untouched.
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::fs::{self, create_dir_all, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub fn get_config_path(filename: &str) -> io::Result<PathBuf> {
    let path = app_dir()?.join(filename);
    if let Some(parent) = path.parent() {
        create_dir_all(parent)?;
    }
    Ok(path)
}

/// The folder every file of the app lives in: the one the app registered
/// through [`crate::init`]. Before that every path fails closed, so nothing is
/// ever read from or written to a directory this crate guessed.
#[cfg(not(test))]
fn app_dir() -> io::Result<PathBuf> {
    crate::app()
        .map(|app| app.data_dir.clone())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "The app has not registered its storage directory",
            )
        })
}

/// Tests get a folder of their own under the system's temp directory, one per
/// run, so `cargo test` never reads, writes or removes a file beside a real
/// wallet's. Every path in the app comes through here.
#[cfg(test)]
fn app_dir() -> io::Result<PathBuf> {
    Ok(std::env::temp_dir().join(format!("dannesk-test-{}", std::process::id())))
}

/// The sibling a write passes through. Same directory, deliberately: `rename` is
/// only atomic *within* one filesystem, so a temp dir elsewhere would silently
/// degrade to a copy.
pub fn tmp_name(filename: &str) -> String {
    format!("{filename}.tmp")
}

pub fn write_json<T: Serialize>(filename: &str, data: &T) -> io::Result<()> {
    let json = serde_json::to_string(data)?;
    write_bytes(filename, json.as_bytes())
}

/// Replace `filename` with `bytes`, atomically — see the module note.
///
/// On any failure the tmp is removed and the original file is left exactly as
/// it was, so a caller that hits an error has nothing to roll back.
pub fn write_bytes(filename: &str, bytes: &[u8]) -> io::Result<()> {
    let path = get_config_path(filename)?;
    let tmp = get_config_path(&tmp_name(filename))?;

    write_through_tmp(&tmp, &path, bytes).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })
}

fn write_through_tmp(tmp: &Path, path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut opts = OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    // Owner-only (rw-------) from the moment the file exists, rather than
    // tightened afterwards: the old code wrote the ciphertext first and chmod'd
    // second, which is a window another process on the same box can read.
    // Contents are ciphertext either way — this is defense in depth. No-op
    // elsewhere.
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }

    let mut file = opts.open(tmp)?;

    // `mode` above only applies when the file is *created*, so a tmp left behind
    // by an interrupted write would keep whatever permissions it already had.
    // Still before a single byte of ciphertext exists.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }

    file.write_all(bytes)?;
    // Without this the rename can land while the data is still in page cache:
    // the directory entry survives a power cut pointing at a file of zeros.
    file.sync_all()?;
    drop(file);

    fs::rename(tmp, path)?;

    // The rename itself is metadata, so it needs the *directory* flushed to be
    // durable. Best effort on purpose: the swap has already happened and the
    // caller's data is in place, so failing here would report a write that
    // actually succeeded — which is how a caller ends up rolling back a good
    // file. Durability degrades to "the filesystem's own schedule"; correctness
    // does not.
    #[cfg(unix)]
    if let Some(dir) = path.parent()
        && let Ok(handle) = File::open(dir)
    {
        let _ = handle.sync_all();
    }

    Ok(())
}

pub fn read_json<T: DeserializeOwned>(filename: &str) -> io::Result<T> {
    let path = get_config_path(filename)?;
    let content = fs::read_to_string(path)?;
    let data = serde_json::from_str(&content)?;
    Ok(data)
}

/// The file's raw bytes, or `None` when it isn't there.
///
/// For callers that need to put a file *back* the way it was — restoring the
/// bytes verbatim, without parsing and re-serializing something they may not
/// fully model.
pub fn read_bytes(filename: &str) -> io::Result<Option<Vec<u8>>> {
    let path = get_config_path(filename)?;
    match fs::read(&path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

pub fn update_json<T: Serialize + DeserializeOwned + std::default::Default>(
    filename: &str,
    update_fn: impl FnOnce(&mut T),
) -> io::Result<()> {
    // Default ONLY when the file genuinely isn't there. The old blanket
    // `or_else` also swallowed parse and permission errors, and for the
    // `serde_json::Value` callers that meant `Value::Null` — whose
    // `as_object_mut()` returns `None`, so every closure here silently did
    // nothing and the literal `null` was then written over a perfectly good
    // wallet file, taking `address` with it.
    let mut data = match read_json(filename) {
        Ok(data) => data,
        Err(e) if e.kind() == io::ErrorKind::NotFound => T::default(),
        Err(e) => return Err(e),
    };
    update_fn(&mut data);
    write_json(filename, &data)?;
    Ok(())
}

/// Delete `filename` and any tmp sibling left by an interrupted write.
///
/// Idempotent: a file that is already gone is a success, not a `NotFound`. The
/// callers all want "make sure this isn't on disk", and every one of them used
/// to spell that as an `exists()` check immediately before the call.
///
/// The tmp goes too. It holds the same ciphertext as the file it was replacing,
/// so leaving one behind would quietly survive a purge — including the gate's
/// five-strike wipe, whose whole contract is that nothing is left.
pub fn remove_json(filename: &str) -> io::Result<()> {
    let path = get_config_path(filename)?;
    let tmp = get_config_path(&tmp_name(filename))?;

    let removed = match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    };

    match fs::remove_file(&tmp) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        // A tmp we cannot delete matters as much as a file we cannot delete:
        // same secret, same directory.
        Err(e) => return Err(e),
    }

    removed
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The whole point of the tmp dance: the destination is either the old file
    /// or the new one. Checked by proving the tmp is gone afterwards and the
    /// bytes are the new ones — a `fs::write` would leave neither guarantee.
    #[test]
    fn a_write_leaves_no_tmp_behind() {
        let name = "test_atomic_write.json";
        let _ = remove_json(name);

        write_json(name, &json!({ "address": "rTest" })).expect("first write");
        write_json(name, &json!({ "address": "rSecond" })).expect("second write");

        let read: serde_json::Value = read_json(name).expect("read back");
        assert_eq!(read["address"], "rSecond");
        assert!(
            !get_config_path(&tmp_name(name)).unwrap().exists(),
            "a completed write must not leave its tmp on disk",
        );

        let _ = remove_json(name);
    }

    /// The bug this replaced: a file that exists but cannot be parsed used to
    /// default to `Value::Null`, whose `as_object_mut()` is `None`, so the
    /// closure did nothing and `null` was written over the wallet.
    #[test]
    fn update_refuses_to_default_over_an_unparseable_file() {
        let name = "test_update_garbage.json";
        write_bytes(name, b"{ this is not json").expect("seed garbage");

        let result = update_json(name, |data: &mut serde_json::Value| {
            if let Some(obj) = data.as_object_mut() {
                obj.insert("touched".to_string(), json!(true));
            }
        });

        assert!(result.is_err(), "a corrupt file must not be overwritten");
        assert_eq!(
            read_bytes(name).unwrap().as_deref(),
            Some(&b"{ this is not json"[..]),
            "the original bytes must survive the failed update",
        );

        let _ = remove_json(name);
    }

    /// A missing file is still the create case — that path has to keep working
    /// or first-run writes break.
    #[test]
    fn update_still_creates_a_missing_file() {
        let name = "test_update_missing.json";
        let _ = remove_json(name);

        update_json(name, |data: &mut serde_json::Value| {
            *data = json!({ "created": true });
        })
        .expect("missing file is the create case");

        let read: serde_json::Value = read_json(name).expect("read back");
        assert_eq!(read["created"], true);

        let _ = remove_json(name);
    }

    /// Removal is "make sure it's gone", including the tmp — the five-strike
    /// wipe promises nothing is left, and a tmp holds the same ciphertext.
    #[test]
    fn remove_is_idempotent_and_takes_the_tmp() {
        let name = "test_remove_tmp.json";
        write_json(name, &json!({ "address": "rTest" })).expect("write");
        fs::write(get_config_path(&tmp_name(name)).unwrap(), b"leftover").expect("plant a tmp");

        remove_json(name).expect("first removal");
        assert!(!get_config_path(name).unwrap().exists());
        assert!(!get_config_path(&tmp_name(name)).unwrap().exists());

        remove_json(name).expect("removing what is already gone is a success");
    }
}
