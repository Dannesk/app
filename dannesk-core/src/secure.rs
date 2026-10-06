//! Locked, zeroizing secret buffers.
//!
//! Holds assembled secret material — argon2-derived keys, raw BIP39 seeds,
//! decrypted mnemonics — in memory that the OS is told never to swap to disk
//! (`mlock` / `VirtualLock`, via the `region` crate) and that is wiped on drop
//! (`zeroize`). Together they close the swap / hibernation / crash-dump leak for
//! these values, which plain `zeroize` alone does not (zeroize only wipes the
//! live RAM copy *after* use — a page can be swapped out before that).
//!
//! Scope / known limit: this covers secrets we assemble in our own backend. The
//! secrets a user types still transit iced's widget buffers, the per-keystroke
//! `Message` payloads, and `AppState` `String` fields — allocations we do not
//! own and therefore cannot lock. This protects the derived crown-jewel values,
//! not the raw keystroke buffers.

use zeroize::Zeroize;

/// Hardens the current process against secret extraction via crash dumps and
/// live-process inspection. Call once, as early in `main` as possible (before
/// any worker threads spawn — these are per-process attributes inherited by
/// every thread).
///
/// - `RLIMIT_CORE = 0` (Linux + macOS): no core file is ever written, so a
///   crash — including the release profile's `panic = "abort"` → `SIGABRT` —
///   cannot spill the mlocked, decrypted secret to disk.
/// - `PR_SET_DUMPABLE = 0` (Linux only): clears the "dumpable" flag, which also
///   restricts `ptrace` and `/proc/<pid>/mem` to root — blocking a same-user
///   malicious process from scraping the live secret during the signing window.
///
/// Limits it cannot close: does not stop `root`; does not stop a full-RAM
/// hibernation image (neither does `mlock`). Windows (WER crash dumps) is not
/// covered here — see backlog.
///
/// Release builds only: dev builds stay debuggable (gdb/lldb attach, cores).
#[cfg(all(unix, not(debug_assertions)))]
pub fn harden_process() {
    unsafe {
        let no_core = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
        let _ = libc::setrlimit(libc::RLIMIT_CORE, &no_core);

        #[cfg(target_os = "linux")]
        {
            let _ = libc::prctl(libc::PR_SET_DUMPABLE, 0 as libc::c_ulong, 0, 0, 0);
        }
    }
}

/// No-op on non-Unix or in debug builds (keeps development debuggable).
#[cfg(any(not(unix), debug_assertions))]
pub fn harden_process() {}

/// A heap byte buffer that is mlocked for its lifetime and zeroized on drop.
///
/// The backing allocation is fixed at construction and never grown, so the lock
/// stays valid for the buffer's whole life. `mlock` operates on whole pages, so
/// a buffer may share its locked page(s) with unrelated heap data — acceptable,
/// since locking extra is harmless.
pub struct SecureBytes {
    buf: Vec<u8>,
    // Unlocks the pages on drop. `None` if the lock could not be acquired (e.g.
    // RLIMIT_MEMLOCK exhausted) — we still zeroize, we just couldn't pin.
    guard: Option<region::LockGuard>,
}

impl SecureBytes {
    /// Take ownership of `bytes` and lock its pages. The vec is moved in as-is
    /// (no reallocation), so an existing `String`/`Vec` allocation is reused.
    pub fn new(bytes: Vec<u8>) -> Self {
        let guard = if bytes.is_empty() {
            None
        } else {
            region::lock(bytes.as_ptr(), bytes.len()).ok()
        };
        SecureBytes { buf: bytes, guard }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.buf
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

}

impl Drop for SecureBytes {
    fn drop(&mut self) {
        self.buf.zeroize();
        // Unlock the (now-zeroed) pages.
        drop(self.guard.take());
    }
}

/// Default reserved (and locked) capacity for an editable input buffer, so that
/// ordinary typing never reallocates — keeping the lock valid and avoiding a
/// freed-but-unwiped buffer on growth. Comfortably fits a 24-word seed phrase.
const INPUT_CAP: usize = 512;

/// Locks the whole *capacity* of `buf` (not just its current length), so bytes
/// written later still land on already-locked pages. `None` if the lock can't
/// be acquired (e.g. RLIMIT_MEMLOCK) — we still zeroize, we just couldn't pin.
fn lock_capacity(buf: &[u8], cap: usize) -> Option<region::LockGuard> {
    if cap == 0 {
        None
    } else {
        region::lock(buf.as_ptr(), cap).ok()
    }
}

/// A locked, zeroizing, *editable* UTF-8 secret buffer — the backing store for
/// secrets the user types (passphrase, BIP39 word, seed phrase). Holds valid
/// UTF-8 at all times; never reallocates via `Vec`'s own path (which would free
/// the old secret bytes unwiped), instead growing manually with wipe + re-lock.
pub struct SecureString {
    buf: Vec<u8>,
    guard: Option<region::LockGuard>,
}

impl SecureString {
    /// Build from an existing `String` (e.g. a freshly decrypted mnemonic).
    /// `into_bytes` reuses the String's allocation — no copy.
    pub fn new(s: String) -> Self {
        let buf = s.into_bytes();
        let guard = lock_capacity(&buf, buf.capacity());
        SecureString { buf, guard }
    }

    /// An empty buffer with reserved, pre-locked capacity for interactive input.
    pub fn input() -> Self {
        let buf = Vec::with_capacity(INPUT_CAP);
        let guard = lock_capacity(&buf, buf.capacity());
        SecureString { buf, guard }
    }

    /// A truly empty buffer — no capacity, nothing locked. Cheap; used as the
    /// placeholder left behind by [`take`](Self::take). Re-locks itself on first
    /// write via `ensure`.
    fn empty() -> Self {
        SecureString { buf: Vec::new(), guard: None }
    }

    /// Move the secret out, leaving an empty buffer in its place. Only the `Vec`
    /// header and lock guard move; the heap allocation (and its `mlock`) stay
    /// put, so the secret bytes are never copied and the lock stays valid across
    /// the move — this is the primitive that threads a secret through the
    /// dispatch boundary without an unlocked copy.
    pub fn take(&mut self) -> SecureString {
        std::mem::replace(self, SecureString::empty())
    }

    /// [`take`](Self::take), with leading and trailing whitespace removed first
    /// — how every typed key and 25th word leaves its field. A pasted secret
    /// routinely brings a stray space at its end; interior spaces are part of
    /// it and stay. Trimmed in place (bytes shifted down, freed tail wiped), so
    /// no unlocked copy is made. Everything downstream — bridges, auth,
    /// derivation, decryption — uses what it is handed, as-is.
    pub fn take_trimmed(&mut self) -> SecureString {
        let (start, len) = {
            let s = self.as_str();
            let t = s.trim();
            (t.as_ptr() as usize - s.as_ptr() as usize, t.len())
        };
        let old_len = self.buf.len();
        self.buf.copy_within(start..start + len, 0);
        for b in &mut self.buf[len..old_len] {
            *b = 0;
        }
        self.buf.truncate(len);
        self.take()
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.buf).unwrap_or("")
    }

    /// [`char_len`](Self::char_len) of what [`take_trimmed`](Self::take_trimmed)
    /// would hand off — what the setup and signing minimums count.
    pub fn trimmed_char_len(&self) -> usize {
        self.as_str().trim().chars().count()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// Number of Unicode scalar values — i.e. how many mask bullets to render.
    pub fn char_len(&self) -> usize {
        self.as_str().chars().count()
    }

    /// Ensure room for `extra` more bytes without a `Vec` reallocation. If we
    /// must grow, copy into a fresh locked buffer and wipe the old one — never
    /// let `Vec` free a buffer still holding secret bytes.
    fn ensure(&mut self, extra: usize) {
        if self.buf.len() + extra <= self.buf.capacity() {
            return;
        }
        let new_cap = (self.buf.len() + extra).max(self.buf.capacity() * 2).max(INPUT_CAP);
        let mut new_buf = Vec::with_capacity(new_cap);
        new_buf.extend_from_slice(&self.buf);
        let new_guard = lock_capacity(&new_buf, new_buf.capacity());
        self.buf.zeroize();
        self.buf = new_buf; // old buf (already wiped) drops here; old guard replaced
        self.guard = new_guard;
    }

    /// Byte offset of the character at `char_idx` (or end of buffer).
    fn byte_index(&self, char_idx: usize) -> usize {
        self.as_str()
            .char_indices()
            .nth(char_idx)
            .map(|(i, _)| i)
            .unwrap_or(self.buf.len())
    }

    /// Insert `bytes` at byte offset `at`, growing without ever letting `Vec`
    /// free a secret-bearing buffer (capacity is ensured first, then the bytes
    /// are shifted in place).
    fn insert_bytes(&mut self, at: usize, bytes: &[u8]) {
        let n = bytes.len();
        if n == 0 {
            return;
        }
        self.ensure(n);
        let old_len = self.buf.len();
        self.buf.resize(old_len + n, 0); // capacity ensured → no realloc
        self.buf.copy_within(at..old_len, at + n);
        self.buf[at..at + n].copy_from_slice(bytes);
    }

    /// Insert a typed character at character position `char_idx`.
    pub fn insert(&mut self, char_idx: usize, c: char) {
        let at = self.byte_index(char_idx);
        let mut tmp = [0u8; 4];
        let s = c.encode_utf8(&mut tmp);
        self.insert_bytes(at, s.as_bytes());
        tmp.zeroize();
    }

    /// Insert pasted text at character position `char_idx`.
    pub fn insert_str(&mut self, char_idx: usize, s: &str) {
        let at = self.byte_index(char_idx);
        self.insert_bytes(at, s.as_bytes());
    }

    /// Remove the character at position `char_idx`, wiping the freed tail bytes.
    pub fn remove(&mut self, char_idx: usize) {
        let start = self.byte_index(char_idx);
        let n = match self.as_str()[start..].chars().next() {
            Some(c) => c.len_utf8(),
            None => return,
        };
        let old_len = self.buf.len();
        self.buf.copy_within(start + n.., start);
        let new_len = old_len - n;
        for b in &mut self.buf[new_len..old_len] {
            *b = 0;
        }
        self.buf.truncate(new_len);
    }

    /// Wipe and empty the buffer (keeps the locked allocation for reuse).
    pub fn clear(&mut self) {
        self.buf.zeroize();
        self.buf.clear();
    }
}

impl Drop for SecureString {
    fn drop(&mut self) {
        self.buf.zeroize();
        drop(self.guard.take());
    }
}

// Redact contents from Debug — a secret must never reach a log or panic message.
impl std::fmt::Debug for SecureString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SecureString(<redacted; {} chars>)", self.char_len())
    }
}

impl std::fmt::Debug for SecureBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SecureBytes(<redacted; {} bytes>)", self.len())
    }
}

#[cfg(test)]
mod tests {
    use super::SecureString;

    #[test]
    fn take_trimmed_trims_the_ends_and_nothing_else() {
        let take = |s: &str| SecureString::new(s.to_string()).take_trimmed().as_str().to_string();
        assert_eq!(take("  1234---55553 \n"), "1234---55553");
        assert_eq!(take("1234   55553"), "1234   55553");
        assert_eq!(take("correct horse battery "), "correct horse battery");
        assert_eq!(take("   "), "");
        assert_eq!(take(""), "");
    }

    #[test]
    fn take_trimmed_empties_the_field_and_leaves_no_tail() {
        let mut field = SecureString::new("  secret  ".to_string());
        let taken = field.take_trimmed();
        assert!(field.is_empty());
        assert_eq!(taken.as_str(), "secret");
        assert_eq!(taken.buf.len(), "secret".len());
    }

    #[test]
    fn trimmed_char_len_ignores_only_the_ends() {
        assert_eq!(SecureString::new("  12 345  ".to_string()).trimmed_char_len(), 6);
        assert_eq!(SecureString::new("      ".to_string()).trimmed_char_len(), 0);
    }
}
