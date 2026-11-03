//! First-party asset import and binary cooking tools.
pub mod cooked;
pub mod glb;
pub mod image;
/// The JSON reader and writer now live in `abe-core`; this keeps `abe_assets::json` working.
pub use abe_core::json;
pub mod mip;
pub mod model;
pub mod png;
pub mod zlib;
use std::io::Read;
pub fn read_asset(path: &std::path::Path) -> Result<Vec<u8>, String> {
    let f = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut bytes = vec![];
    f.take(cooked::MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > cooked::MAX_BYTES {
        return Err("asset exceeds file size budget".into());
    }
    Ok(bytes)
}

/// Replace an output only after its complete bytes are on disk. A failed cook leaves the
/// prior asset usable, and concurrent invocations never share a partial file.
pub fn write_asset(path: &std::path::Path, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let mut retries = 0;
    loop {
        let tmp = dir.join(format!(
            ".abe-cook-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let f = match std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&tmp)
        {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && retries < 16 => {
                retries += 1;
                continue;
            }
            Err(e) => return Err(e),
        };
        // The file closes at the end of this block, before the rename.
        let res = {
            let mut f = f;
            f.write_all(data).and_then(|()| f.sync_all())
        };
        let res = res.and_then(|()| std::fs::rename(&tmp, path));
        if res.is_err() {
            let _ = std::fs::remove_file(tmp);
        }
        return res;
    }
}
