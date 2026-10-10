//! The `.jup4x` file Jup-8 V writes with "Export Preset/Bank" and reads with "Import": a ZIP whose
//! entries are preset files at `<product>/User/<bank>/<preset>`. Jup-8 V's importer only accepts a
//! minimal container: stored entries, no flags, no extra fields, no directory entries.

use super::arturia::ArturiaPreset;
use std::collections::HashSet;
use std::fmt;

pub const FILE_EXTENSION: &str = "jup4x";
/// Product folder inside the archive (the instrument's display name in Arturia's database).
pub const PRODUCT_FOLDER: &str = "Jup-8";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ZipError {
    NotAZip,
    UnsupportedCompression(u16),
    Corrupt,
}

impl fmt::Display for ZipError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ZipError::NotAZip => write!(f, "not a ZIP archive"),
            ZipError::UnsupportedCompression(m) => write!(f, "unsupported ZIP compression method {m}"),
            ZipError::Corrupt => write!(f, "the archive is damaged"),
        }
    }
}

impl std::error::Error for ZipError {}

pub fn sanitize(s: &str) -> String {
    let t = s.replace(['/', '\\', ':'], "-");
    let mut t = t.trim().to_string();
    while t.starts_with('.') {
        t.remove(0);
    }
    if t.is_empty() {
        "Untitled".to_string()
    } else {
        t
    }
}

/// Builds an export archive; the bank folder is each preset's `pack`.
pub fn make(presets: &[ArturiaPreset]) -> Vec<u8> {
    let mut zip = MinimalZip::default();
    let mut used = HashSet::new();
    for p in presets {
        let bank = sanitize(if p.pack.is_empty() { "User" } else { &p.pack });
        let base = sanitize(if p.name.is_empty() { "Untitled" } else { &p.name });
        let mut name = base.clone();
        let mut path = format!("{PRODUCT_FOLDER}/User/{bank}/{name}");
        let mut n = 2;
        while used.contains(&path) {
            name = format!("{base} {n}");
            path = format!("{PRODUCT_FOLDER}/User/{bank}/{name}");
            n += 1;
        }
        used.insert(path.clone());
        zip.add(&path, &p.serialized());
    }
    zip.finish()
}

/// Reads every preset in an archive (any folder layout). Also accepts a bare preset file.
pub fn read(data: &[u8]) -> Result<Vec<ArturiaPreset>, Box<dyn std::error::Error + Send + Sync>> {
    if ArturiaPreset::looks_like_preset(data) {
        return Ok(vec![ArturiaPreset::parse(data, false)?]);
    }
    let mut out = Vec::new();
    for (path, d) in entries(data)? {
        if !ArturiaPreset::looks_like_preset(&d) {
            continue;
        }
        if let Ok(mut p) = ArturiaPreset::parse(&d, false) {
            let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
            if p.pack.is_empty() && parts.len() >= 2 {
                p.pack = parts[parts.len() - 2].to_string();
            }
            out.push(p);
        }
    }
    Ok(out)
}

/// Minimal ZIP writer (stored entries).
#[derive(Default)]
struct MinimalZip {
    body: Vec<u8>,
    central: Vec<u8>,
    count: usize,
}

fn le16(v: &mut Vec<u8>, x: u16) {
    v.extend_from_slice(&x.to_le_bytes());
}

fn le32(v: &mut Vec<u8>, x: u32) {
    v.extend_from_slice(&x.to_le_bytes());
}

impl MinimalZip {
    fn add(&mut self, path: &str, data: &[u8]) {
        let name = path.as_bytes();
        let crc = crc32(data);
        let offset = self.body.len() as u32;
        let b = &mut self.body;
        le32(b, 0x0403_4B50);
        le16(b, 20);
        le16(b, 0);
        le16(b, 0);
        le16(b, 0);
        le16(b, 0x21); // time 00:00, date 1980-01-01
        le32(b, crc);
        le32(b, data.len() as u32);
        le32(b, data.len() as u32);
        le16(b, name.len() as u16);
        le16(b, 0);
        b.extend_from_slice(name);
        b.extend_from_slice(data);

        let c = &mut self.central;
        le32(c, 0x0201_4B50);
        le16(c, 20);
        le16(c, 20);
        le16(c, 0);
        le16(c, 0);
        le16(c, 0);
        le16(c, 0x21);
        le32(c, crc);
        le32(c, data.len() as u32);
        le32(c, data.len() as u32);
        le16(c, name.len() as u16);
        le16(c, 0);
        le16(c, 0);
        le16(c, 0);
        le16(c, 0);
        le32(c, 0);
        le32(c, offset);
        c.extend_from_slice(name);
        self.count += 1;
    }

    fn finish(self) -> Vec<u8> {
        let mut out = self.body.clone();
        out.extend_from_slice(&self.central);
        le32(&mut out, 0x0605_4B50);
        le16(&mut out, 0);
        le16(&mut out, 0);
        le16(&mut out, self.count as u16);
        le16(&mut out, self.count as u16);
        le32(&mut out, self.central.len() as u32);
        le32(&mut out, self.body.len() as u32);
        le16(&mut out, 0);
        out
    }
}

/// Lists the file entries of a ZIP (stored or deflated).
pub fn entries(zip: &[u8]) -> Result<Vec<(String, Vec<u8>)>, ZipError> {
    let b = zip;
    let u16_at = |o: usize| -> usize { b[o] as usize | (b[o + 1] as usize) << 8 };
    let u32_at = |o: usize| -> usize { u16_at(o) | u16_at(o + 2) << 16 };
    if b.len() < 22 {
        return Err(ZipError::NotAZip);
    }
    // end of central directory
    let mut eocd = None;
    let mut i = b.len() - 22;
    let lowest = b.len().saturating_sub(22 + 65_535);
    loop {
        if u32_at(i) == 0x0605_4B50 {
            eocd = Some(i);
            break;
        }
        if i == lowest || i == 0 {
            break;
        }
        i -= 1;
    }
    let eocd = eocd.ok_or(ZipError::NotAZip)?;
    let total = u16_at(eocd + 10);
    let mut p = u32_at(eocd + 16);
    let mut out = Vec::new();
    for _ in 0..total {
        if p + 46 > b.len() || u32_at(p) != 0x0201_4B50 {
            return Err(ZipError::Corrupt);
        }
        let method = u16_at(p + 10) as u16;
        let csize = u32_at(p + 20);
        let usize_ = u32_at(p + 24);
        let (nlen, xlen, clen, local) = (u16_at(p + 28), u16_at(p + 30), u16_at(p + 32), u32_at(p + 42));
        if p + 46 + nlen > b.len() {
            return Err(ZipError::Corrupt);
        }
        let name = String::from_utf8_lossy(&b[p + 46..p + 46 + nlen]).into_owned();
        p += 46 + nlen + xlen + clen;
        if local + 30 > b.len() || u32_at(local) != 0x0403_4B50 {
            return Err(ZipError::Corrupt);
        }
        let start = local + 30 + u16_at(local + 26) + u16_at(local + 28);
        if start + csize > b.len() {
            return Err(ZipError::Corrupt);
        }
        if name.ends_with('/') {
            continue;
        }
        let raw = &b[start..start + csize];
        match method {
            0 => out.push((name, raw.to_vec())),
            8 => {
                let d = miniz_oxide::inflate::decompress_to_vec(raw).map_err(|_| ZipError::Corrupt)?;
                if d.len() != usize_ {
                    return Err(ZipError::Corrupt);
                }
                out.push((name, d));
            }
            m => return Err(ZipError::UnsupportedCompression(m)),
        }
    }
    Ok(out)
}

pub fn crc32(data: &[u8]) -> u32 {
    use std::sync::OnceLock;
    static TABLE: OnceLock<[u32; 256]> = OnceLock::new();
    let table = TABLE.get_or_init(|| {
        let mut t = [0u32; 256];
        for (i, e) in t.iter_mut().enumerate() {
            let mut c = i as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            }
            *e = c;
        }
        t
    });
    let mut c = 0xFFFF_FFFFu32;
    for &byte in data {
        c = table[((c ^ byte as u32) & 0xFF) as usize] ^ (c >> 8);
    }
    c ^ 0xFFFF_FFFF
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zip_round_trip() {
        let mut p = ArturiaPreset::new("Test / One", "My Bank");
        p.params.insert("LpfCut".into(), 0.5);
        let z = make(&[p.clone(), p.clone()]);
        let e = entries(&z).unwrap();
        assert_eq!(e.len(), 2);
        assert_eq!(e[0].0, "Jup-8/User/My Bank/Test - One");
        assert_eq!(e[1].0, "Jup-8/User/My Bank/Test - One 2");
        let back = read(&z).unwrap();
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].params["LpfCut"], 0.5);
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }
}
