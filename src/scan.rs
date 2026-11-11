//! Disk-usage measurement: allocated bytes on disk, the same idea as `du`.
//!
//! * Symlinks are never followed and count as 0.
//! * Hard-linked files are counted once per [`disk_usage`] call.
//! * Unreadable entries (permission denied, vanished while scanning) count as 0.
//! * A directory's own metadata blocks are not counted (they are 0 on APFS anyway).
//!
//! Subdirectories are scanned in parallel on rayon's work-stealing pool, while
//! the files of one directory are summed in place, so no task is spawned per file.
//!
//! On macOS the scanner uses `getattrlistbulk(2)`, which returns the type, size and
//! inode of a whole batch of directory entries in one syscall instead of
//! one `lstat` per file (about 1.2–1.5× faster on large caches). The remaining
//! cost is the kernel fetching file metadata, which no API can avoid.

use std::collections::HashSet;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::sync::Mutex;

/// Allocated size of `path` on disk, in bytes, summed recursively.
pub fn disk_usage(path: &Path) -> u64 {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return 0;
    };
    let file_type = meta.file_type();
    if file_type.is_symlink() {
        return 0;
    }
    if !file_type.is_dir() {
        return allocated(&meta);
    }
    dir_usage(path, &HardLinks::default())
}

#[cfg(target_os = "macos")]
use bulk::dir_usage;
#[cfg(not(target_os = "macos"))]
use portable::dir_usage;

/// `st_blocks` is always in 512-byte units, whatever the filesystem block size.
fn allocated(meta: &fs::Metadata) -> u64 {
    meta.blocks() * 512
}

/// Tracks files with several hard links, so each one is counted only once.
/// Only multi-link files touch the lock, which keeps contention negligible.
#[derive(Default)]
struct HardLinks(Mutex<HashSet<(u64, u64)>>);

impl HardLinks {
    fn first_visit(&self, dev: u64, ino: u64) -> bool {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert((dev, ino))
    }
}

/// Portable implementation on top of `std::fs`: one `lstat` per entry.
/// Also compiled into macOS tests, where it serves as the reference implementation.
#[cfg(any(test, not(target_os = "macos")))]
mod portable {
    use super::{HardLinks, allocated};
    use rayon::prelude::*;
    use std::fs;
    use std::os::unix::fs::MetadataExt;
    use std::path::Path;

    pub(super) fn dir_usage(dir: &Path, seen: &HardLinks) -> u64 {
        let Ok(entries) = fs::read_dir(dir) else {
            return 0;
        };
        let mut files = 0;
        let mut subdirs = Vec::new();
        for entry in entries.flatten() {
            // `file_type` comes from readdir itself, so symlinks are skipped without a stat.
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                subdirs.push(entry.path());
                continue;
            }
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if meta.nlink() <= 1 || seen.first_visit(meta.dev(), meta.ino()) {
                files += allocated(&meta);
            }
        }
        files + subdirs.par_iter().map(|d| dir_usage(d, seen)).sum::<u64>()
    }
}

/// macOS implementation on top of `getattrlistbulk(2)`.
#[cfg(target_os = "macos")]
mod bulk {
    use super::HardLinks;
    use rayon::prelude::*;
    use std::cell::RefCell;
    use std::ffi::{CString, OsStr, c_int};
    use std::os::unix::ffi::OsStrExt;
    use std::path::{Path, PathBuf};

    /// Not exported by the `libc` crate.
    const ATTR_CMN_ERROR: u32 = 0x2000_0000;
    /// `fsobj_type_t` values from `<sys/vnode.h>`.
    const VREG: u32 = 1;
    const VDIR: u32 = 2;
    const VLNK: u32 = 5;
    /// Bytes of attribute buffer per thread; holds a few hundred entries per syscall.
    const BUF_BYTES: usize = 128 * 1024;

    thread_local! {
        // `u64` elements keep the buffer 8-byte aligned for the kernel.
        static BUF: RefCell<Vec<u64>> = RefCell::new(vec![0; BUF_BYTES / 8]);
    }

    pub(super) fn dir_usage(dir: &Path, seen: &HardLinks) -> u64 {
        let mut subdirs = Vec::new();
        let files = read_dir(dir, seen, &mut subdirs);
        // The directory fd and the thread-local buffer are released before
        // recursing, so open fds stay bounded by the thread count and a stolen
        // task on this thread can borrow the buffer again.
        files + subdirs.par_iter().map(|d| dir_usage(d, seen)).sum::<u64>()
    }

    /// Sums the files directly inside `dir` and collects its subdirectories.
    fn read_dir(dir: &Path, seen: &HardLinks, subdirs: &mut Vec<PathBuf>) -> u64 {
        let Some(fd) = Fd::open_dir(dir) else {
            return 0;
        };
        // SAFETY: `attrlist` is a plain C struct; all-zero is a valid value.
        let mut request: libc::attrlist = unsafe { std::mem::zeroed() };
        request.bitmapcount = libc::ATTR_BIT_MAP_COUNT;
        request.commonattr = libc::ATTR_CMN_RETURNED_ATTRS
            | ATTR_CMN_ERROR
            | libc::ATTR_CMN_NAME
            | libc::ATTR_CMN_DEVID
            | libc::ATTR_CMN_OBJTYPE
            | libc::ATTR_CMN_FILEID;
        request.fileattr = libc::ATTR_FILE_LINKCOUNT | libc::ATTR_FILE_ALLOCSIZE;

        BUF.with_borrow_mut(|buf| {
            let mut total = 0;
            loop {
                // SAFETY: `request` and `buf` are valid for the duration of the call,
                // and the size passed matches the buffer's length in bytes.
                let count = unsafe {
                    libc::getattrlistbulk(
                        fd.0,
                        (&raw mut request).cast(),
                        buf.as_mut_ptr().cast(),
                        buf.len() * 8,
                        0,
                    )
                };
                if count <= 0 {
                    return total; // 0 = end of directory, -1 = error
                }
                // SAFETY: the buffer is initialised memory of exactly this many bytes.
                let bytes: &[u8] =
                    unsafe { std::slice::from_raw_parts(buf.as_ptr().cast(), buf.len() * 8) };
                let mut offset = 0;
                for _ in 0..count {
                    let Some(len) = read_u32(bytes, offset).map(|l| l as usize) else {
                        return total;
                    };
                    let Some(record) = bytes.get(offset..offset + len).filter(|_| len > 0) else {
                        return total;
                    };
                    match parse(record) {
                        Some(Entry::Dir(name)) => subdirs.push(dir.join(name)),
                        Some(Entry::File { dev, ino, links, size })
                            if links <= 1 || seen.first_visit(dev, ino) =>
                        {
                            total += size;
                        }
                        Some(Entry::File { .. }) | None => {}
                    }
                    offset += len;
                }
            }
        })
    }

    enum Entry<'a> {
        Dir(&'a OsStr),
        File { dev: u64, ino: u64, links: u32, size: u64 },
    }

    /// Decodes one record. Attributes are packed in bitmap order, each 4-byte
    /// aligned, and only those listed in the record's returned-attributes set are
    /// present. Returns `None` for symlinks, errored or malformed entries.
    fn parse(record: &[u8]) -> Option<Entry<'_>> {
        let mut r = Reader { buf: record, pos: 4 }; // skip the record length
        let common = r.u32()?;
        r.skip(4 * 2)?; // volattr, dirattr
        let file = r.u32()?;
        r.skip(4)?; // forkattr

        if common & ATTR_CMN_ERROR != 0 && r.u32()? != 0 {
            return None;
        }
        let mut name: &[u8] = &[];
        if common & libc::ATTR_CMN_NAME != 0 {
            let at = r.pos;
            let data_offset = r.u32()?.cast_signed();
            let data_len = r.u32()? as usize;
            let start = at.checked_add_signed(data_offset as isize)?;
            // `data_len` includes the trailing NUL.
            name = record.get(start..start + data_len.checked_sub(1)?)?;
        }
        let dev = if common & libc::ATTR_CMN_DEVID != 0 { r.u32()? } else { 0 };
        let obj_type = if common & libc::ATTR_CMN_OBJTYPE != 0 { r.u32()? } else { 0 };
        let ino = if common & libc::ATTR_CMN_FILEID != 0 { r.u64()? } else { 0 };

        match obj_type {
            VDIR if !name.is_empty() => Some(Entry::Dir(OsStr::from_bytes(name))),
            VDIR | VLNK => None,
            _ => {
                let links = if file & libc::ATTR_FILE_LINKCOUNT != 0 { r.u32()? } else { 1 };
                let size = if file & libc::ATTR_FILE_ALLOCSIZE != 0 { r.u64()? } else { 0 };
                // Inode-based dedup is only meaningful for regular files.
                let links = if obj_type == VREG { links } else { 1 };
                Some(Entry::File { dev: dev.into(), ino, links, size })
            }
        }
    }

    fn read_u32(buf: &[u8], at: usize) -> Option<u32> {
        Some(u32::from_ne_bytes(buf.get(at..at + 4)?.try_into().ok()?))
    }

    /// Bounds-checked, alignment-agnostic cursor over a record.
    struct Reader<'a> {
        buf: &'a [u8],
        pos: usize,
    }

    impl Reader<'_> {
        fn skip(&mut self, n: usize) -> Option<()> {
            self.pos = self.pos.checked_add(n).filter(|&p| p <= self.buf.len())?;
            Some(())
        }
        fn u32(&mut self) -> Option<u32> {
            let v = read_u32(self.buf, self.pos)?;
            self.pos += 4;
            Some(v)
        }
        fn u64(&mut self) -> Option<u64> {
            let bytes = self.buf.get(self.pos..self.pos + 8)?;
            self.pos += 8;
            Some(u64::from_ne_bytes(bytes.try_into().ok()?))
        }
    }

    /// Owned file descriptor, closed on drop.
    struct Fd(c_int);

    impl Fd {
        fn open_dir(path: &Path) -> Option<Self> {
            let c_path = CString::new(path.as_os_str().as_bytes()).ok()?;
            let flags = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
            // SAFETY: `c_path` is a valid NUL-terminated string.
            let fd = unsafe { libc::open(c_path.as_ptr(), flags) };
            (fd >= 0).then_some(Self(fd))
        }
    }

    impl Drop for Fd {
        fn drop(&mut self) {
            // SAFETY: we own this descriptor and close it exactly once.
            unsafe { libc::close(self.0) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    use std::path::PathBuf;

    fn temp_tree(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("rcache-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("a/b/c")).unwrap();
        fs::create_dir_all(root.join("with space/ünïcode")).unwrap();
        fs::write(root.join("top.bin"), vec![1u8; 10_000]).unwrap();
        fs::write(root.join("a/one.bin"), vec![2u8; 70_000]).unwrap();
        fs::write(root.join("a/b/c/deep.bin"), vec![3u8; 300_000]).unwrap();
        fs::write(root.join("with space/ünïcode/x.bin"), vec![4u8; 5_000]).unwrap();
        for i in 0..1_000 {
            // Enough entries to need several getattrlistbulk calls.
            fs::write(root.join(format!("a/b/many-{i:04}-{}.txt", "n".repeat(100))), b"x").unwrap();
        }
        root
    }

    #[test]
    fn counts_hard_links_once_and_ignores_symlinks() {
        let root = temp_tree("links");
        let before = disk_usage(&root);
        fs::hard_link(root.join("a/b/c/deep.bin"), root.join("hard.bin")).unwrap();
        symlink(root.join("a"), root.join("link-to-a")).unwrap();
        assert_eq!(disk_usage(&root), before);
        assert_eq!(disk_usage(&root.join("link-to-a")), 0);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn matches_per_file_metadata() {
        let root = temp_tree("reference");
        let expected = portable::dir_usage(&root, &HardLinks::default());
        assert!(expected >= 385_000, "tree is at least as big as its contents");
        assert_eq!(disk_usage(&root), expected);
        assert_eq!(disk_usage(&root.join("top.bin")), allocated(&fs::metadata(root.join("top.bin")).unwrap()));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn missing_path_is_zero() {
        assert_eq!(disk_usage(Path::new("/definitely/not/here")), 0);
    }
}
