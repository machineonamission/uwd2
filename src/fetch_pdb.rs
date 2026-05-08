use std::io::Read;

pub fn build_url(guid: String) -> String {
    format!("http://msdl.microsoft.com/download/symbols/shell32.pdb/{guid}/shell32.pdb")
}

/// Returns None if the symbol server responds 404 (PDB not yet uploaded).
/// Panics on any other network or HTTP error.
pub fn try_fetch(url: String) -> Option<Vec<u8>> {
    let resp = match ureq::get(url.as_str()).call() {
        Ok(r) => r,
        Err(ureq::Error::Status(404, _)) => {
            println!("Symbol server returned 404 — PDB not yet available.");
            return None;
        }
        Err(e) => panic!("Failed to fetch PDB: {e}"),
    };

    let len: usize = if resp.has("Content-Length") {
        resp.header("Content-Length").unwrap().parse().unwrap()
    } else {
        // last time i checked, the file was about 11.6MB, so this should be fine
        15_000_000
    };

    let mut buf: Vec<u8> = Vec::with_capacity(len);
    resp.into_reader()
        .take(u64::MAX)
        .read_to_end(&mut buf)
        .unwrap();
    Some(buf)
}
