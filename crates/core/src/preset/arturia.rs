//! A Jup-8 V preset file: a Boost text archive with a header, a string map of browser metadata,
//! a map of normalized parameter values and a map of binary blobs (MIDI mappings).
//!
//! Layout (single line, blank separated; strings are `<length> <bytes>`):
//! ```text
//! 22 serialization::archive 10 0 7 0 7 <name> <pack> <instrument id> <author> <type>
//! 0 0 <tag count> 0 <tags...> 1 0 <comment> <unix time> <app version>
//! 0 0 0 0 0 0 0 <empty> 0 0 <meta count> 0 0 0 (<key> <value>)...
//! 0 0 0 7 0 0 0 0 0 0 <param count> 0 0 0 (<name> <value>)... <blob count> 0 (<key> <blob>)... \n
//! ```

use super::boost::{BoostError, BoostReader, BoostWriter};
use std::collections::BTreeMap;
use std::fmt;

pub const JUP8_INSTRUMENT_ID: i64 = 58;
/// Written into new presets; selects the newest value mappings in Jup-8 V (>= 3.9.4).
pub const DEFAULT_VERSION: &str = "4.6.2.5305";

#[derive(Debug, Clone, PartialEq)]
pub struct ArturiaPreset {
    pub name: String,
    pub pack: String,
    pub instrument_id: i64,
    pub author: String,
    pub kind: String,
    pub tags: Vec<String>,
    pub comment: String,
    pub timestamp: i64,
    pub version: String,
    pub meta: BTreeMap<String, String>,
    pub params: BTreeMap<String, f64>,
    pub blobs: BTreeMap<String, Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    NotAnArchive,
    NoParameterMap,
    WrongInstrument(i64),
    Archive(BoostError),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::NotAnArchive => write!(f, "not a Jup-8 V preset (missing Boost archive header)"),
            ParseError::NoParameterMap => write!(f, "no parameter block found in the preset"),
            ParseError::WrongInstrument(id) => write!(f, "preset belongs to another Arturia instrument (id {id})"),
            ParseError::Archive(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ParseError {}

impl From<BoostError> for ParseError {
    fn from(e: BoostError) -> Self {
        ParseError::Archive(e)
    }
}

type Tail = (BTreeMap<String, f64>, BTreeMap<String, Vec<u8>>);

pub fn now_unix() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

impl ArturiaPreset {
    pub fn new(name: &str, pack: &str) -> Self {
        ArturiaPreset {
            name: name.to_string(),
            pack: pack.to_string(),
            instrument_id: JUP8_INSTRUMENT_ID,
            author: String::new(),
            kind: String::new(),
            tags: Vec::new(),
            comment: String::new(),
            timestamp: now_unix(),
            version: DEFAULT_VERSION.to_string(),
            meta: BTreeMap::new(),
            params: BTreeMap::new(),
            blobs: BTreeMap::new(),
        }
    }

    pub fn looks_like_preset(data: &[u8]) -> bool {
        data.starts_with(b"22 serialization::archive 10 0 7")
    }

    /// Parses a preset file. Presets of other Arturia instruments are rejected unless `any_instrument`.
    pub fn parse(data: &[u8], any_instrument: bool) -> Result<ArturiaPreset, ParseError> {
        if !Self::looks_like_preset(data) {
            return Err(ParseError::NotAnArchive);
        }
        let mut r = BoostReader::new(data, 0);
        r.string()?; // "serialization::archive"
        r.skip(5)?; // 10 0 7 0 7
        let mut p = ArturiaPreset::new(&r.string()?, "");
        p.pack = r.string()?;
        p.instrument_id = r.int()?;
        if !any_instrument && p.instrument_id != JUP8_INSTRUMENT_ID {
            return Err(ParseError::WrongInstrument(p.instrument_id));
        }
        p.author = r.string()?;
        p.kind = r.string()?;
        r.skip(2)?;
        let tag_count = r.int()?;
        r.skip(1)?;
        for _ in 0..tag_count.max(0) {
            p.tags.push(r.string()?);
        }
        r.skip(2)?;
        p.comment = r.string()?;
        p.timestamp = r.int()?;
        p.version = r.string()?;
        r.skip(7)?;
        r.data()?;
        r.skip(2)?;
        let meta_count = r.int()?;
        r.skip(if meta_count > 0 { 3 } else { 1 })?;
        for _ in 0..meta_count.max(0) {
            let k = r.string()?;
            let v = r.string()?;
            p.meta.insert(k, v);
        }
        // The parameter map normally follows "0 0 0 7 0 0 0 0 0 0"; layered presets embed extra
        // sub-headers there, so search for the map that parses cleanly up to the end of the file.
        let (params, blobs) = find_parameter_map(data, r.pos, meta_count == 0).ok_or(ParseError::NoParameterMap)?;
        p.params = params;
        p.blobs = blobs;
        Ok(p)
    }

    /// Serializes in the layout Jup-8 V writes (maps sorted by key, like std::map).
    pub fn serialized(&self) -> Vec<u8> {
        let mut w = BoostWriter::default();
        w.string("serialization::archive");
        w.ints(&[10, 0, 7, 0, 7]);
        w.string(&self.name);
        w.string(&self.pack);
        w.int(self.instrument_id);
        w.string(&self.author);
        w.string(&self.kind);
        w.ints(&[0, 0, self.tags.len() as i64, 0]);
        for t in &self.tags {
            w.string(t);
        }
        w.ints(&[1, 0]);
        w.string(&self.comment);
        w.int(self.timestamp);
        w.string(&self.version);
        w.ints(&[0, 0, 0, 0, 0, 0, 0]);
        w.string("");
        w.ints(&[0, 0]);
        let mut meta = self.meta.clone();
        if meta.is_empty() {
            // keeps the pair class header where Jup-8 V expects it
            meta.insert("Type".into(), "Custom".into());
        }
        w.ints(&[meta.len() as i64, 0, 0, 0]);
        for (k, v) in &meta {
            w.string(k);
            w.string(v);
        }
        w.ints(&[0, 0, 0, 7, 0, 0, 0, 0, 0, 0]);
        w.ints(&[self.params.len() as i64, 0, 0, 0]);
        for (k, v) in &self.params {
            w.string(k);
            w.float(*v);
        }
        w.ints(&[self.blobs.len() as i64, 0]);
        for (k, v) in &self.blobs {
            w.string(k);
            w.bytes(v);
        }
        w.finish()
    }

    /// Version-aware decode: presets saved before 3.9.4 used shorter ranges for a few parameters.
    /// Returns the value re-expressed in the current mapping.
    pub fn normalized_for_current_version(&self, id: &str) -> Option<f64> {
        let v = *self.params.get(id)?;
        if !version_less(&self.version, "3.9.4") {
            return Some(v);
        }
        match id {
            "LFO_FadeDelay" => {
                // 0...3.8 s in old presets, 0...7.5 s now (same Exp(2.3) curve)
                let seconds = 3.8 * crate::params::Curve::Exp(2.3).shape(v);
                Some(crate::params::spec(id).normalized_from_physical(seconds))
            }
            _ => Some(v),
        }
    }
}

pub fn version_less(a: &str, b: &str) -> bool {
    let pa: Vec<i64> = a.split('.').map(|x| x.parse().unwrap_or(0)).collect();
    let pb: Vec<i64> = b.split('.').map(|x| x.parse().unwrap_or(0)).collect();
    for i in 0..pa.len().max(pb.len()) {
        let x = pa.get(i).copied().unwrap_or(0);
        let y = pb.get(i).copied().unwrap_or(0);
        if x != y {
            return x < y;
        }
    }
    false
}

fn find_parameter_map(b: &[u8], start: usize, meta_was_empty: bool) -> Option<Tail> {
    let mut i = start.max(1);
    let digit = |c: u8| c.is_ascii_digit();
    while i + 8 < b.len() {
        // candidate: "<digits> 0 0 0 <digit>" preceded by a blank
        if b[i - 1] == 0x20 && digit(b[i]) {
            let mut j = i;
            while j < b.len() && digit(b[j]) {
                j += 1;
            }
            if j + 7 < b.len()
                && b[j] == 0x20
                && b[j + 1] == 0x30
                && b[j + 2] == 0x20
                && b[j + 3] == 0x30
                && b[j + 4] == 0x20
                && b[j + 5] == 0x30
                && b[j + 6] == 0x20
            {
                if let Some(t) = try_parse_tail(b, i, meta_was_empty) {
                    return Some(t);
                }
            }
            i = j;
        }
        i += 1;
    }
    None
}

fn try_parse_tail(b: &[u8], offset: usize, meta_was_empty: bool) -> Option<Tail> {
    let headers: [usize; 2] = if meta_was_empty { [3, 1] } else { [1, 3] };
    for blob_header in headers {
        let mut r = BoostReader::new(b, offset);
        let attempt = (|| -> Result<Option<Tail>, BoostError> {
            let n = r.int()?;
            if n <= 0 || n >= 100_000 {
                return Ok(None);
            }
            r.skip(3)?;
            let mut params = BTreeMap::new();
            for _ in 0..n {
                let k = r.string()?;
                let v = r.double()?;
                params.insert(k, v);
            }
            let m = r.int()?;
            if !(0..100_000).contains(&m) {
                return Ok(None);
            }
            r.skip(blob_header)?;
            let mut blobs = BTreeMap::new();
            for _ in 0..m {
                let k = r.string()?;
                let d = r.data()?.to_vec();
                blobs.insert(k, d);
            }
            if r.is_at_end() {
                Ok(Some((params, blobs)))
            } else {
                Err(BoostError::UnexpectedEnd(r.pos))
            }
        })();
        match attempt {
            Ok(Some(t)) => return Some(t),
            Ok(None) => return None,
            Err(_) => continue,
        }
    }
    None
}
