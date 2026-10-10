//! Reader/writer for the subset of the Boost.Serialization text archive format used by Arturia
//! presets: blank-separated number tokens and length-prefixed byte strings (`<length> <bytes>`,
//! exactly one blank after the length; the bytes may contain anything).

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoostError {
    UnexpectedEnd(usize),
    NotANumber(String, usize),
}

impl fmt::Display for BoostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BoostError::UnexpectedEnd(at) => write!(f, "unexpected end of archive at byte {at}"),
            BoostError::NotANumber(t, at) => write!(f, "expected a number, found '{t}' at byte {at}"),
        }
    }
}

impl std::error::Error for BoostError {}

pub struct BoostReader<'a> {
    bytes: &'a [u8],
    pub pos: usize,
}

#[inline]
fn is_blank(b: u8) -> bool {
    b == 0x20 || b == 0x0A || b == 0x0D
}

impl<'a> BoostReader<'a> {
    pub fn new(bytes: &'a [u8], offset: usize) -> Self {
        BoostReader { bytes, pos: offset }
    }

    pub fn is_at_end(&self) -> bool {
        let mut p = self.pos;
        while p < self.bytes.len() && is_blank(self.bytes[p]) {
            p += 1;
        }
        p >= self.bytes.len()
    }

    pub fn token(&mut self) -> Result<&'a str, BoostError> {
        while self.pos < self.bytes.len() && is_blank(self.bytes[self.pos]) {
            self.pos += 1;
        }
        if self.pos >= self.bytes.len() {
            return Err(BoostError::UnexpectedEnd(self.pos));
        }
        let start = self.pos;
        while self.pos < self.bytes.len() && self.bytes[self.pos] != 0x20 && self.bytes[self.pos] != 0x0A {
            self.pos += 1;
        }
        Ok(std::str::from_utf8(&self.bytes[start..self.pos]).unwrap_or(""))
    }

    pub fn int(&mut self) -> Result<i64, BoostError> {
        let at = self.pos;
        let t = self.token()?;
        t.parse::<i64>().map_err(|_| BoostError::NotANumber(t.to_string(), at))
    }

    pub fn double(&mut self) -> Result<f64, BoostError> {
        let at = self.pos;
        let t = self.token()?;
        t.parse::<f64>().map_err(|_| BoostError::NotANumber(t.to_string(), at))
    }

    pub fn skip(&mut self, n: usize) -> Result<(), BoostError> {
        for _ in 0..n {
            self.token()?;
        }
        Ok(())
    }

    pub fn data(&mut self) -> Result<&'a [u8], BoostError> {
        let n = self.int()?;
        if n < 0 {
            return Err(BoostError::NotANumber(n.to_string(), self.pos));
        }
        let n = n as usize;
        self.pos += 1; // the single separating blank
        if self.pos + n > self.bytes.len() {
            return Err(BoostError::UnexpectedEnd(self.pos));
        }
        let d = &self.bytes[self.pos..self.pos + n];
        self.pos += n;
        Ok(d)
    }

    pub fn string(&mut self) -> Result<String, BoostError> {
        Ok(String::from_utf8_lossy(self.data()?).into_owned())
    }
}

/// Like Boost's text_oarchive, every item after the first is preceded by one blank and the
/// archive ends with a line feed.
#[derive(Default)]
pub struct BoostWriter {
    pub data: Vec<u8>,
    started: bool,
}

impl BoostWriter {
    fn item(&mut self, s: &str) {
        if self.started {
            self.data.push(0x20);
        }
        self.started = true;
        self.data.extend_from_slice(s.as_bytes());
    }

    pub fn int(&mut self, v: i64) {
        self.item(&v.to_string());
    }

    pub fn ints(&mut self, v: &[i64]) {
        for &x in v {
            self.int(x);
        }
    }

    /// Floats are written like C++ streams with 8 significant digits ("0.33333334", "1", "0").
    pub fn float(&mut self, v: f64) {
        self.item(&format_float(v));
    }

    pub fn string(&mut self, s: &str) {
        self.bytes(s.as_bytes());
    }

    pub fn bytes(&mut self, d: &[u8]) {
        self.item(&d.len().to_string());
        self.data.push(0x20);
        self.data.extend_from_slice(d);
    }

    pub fn finish(mut self) -> Vec<u8> {
        self.data.push(0x0A);
        self.data
    }
}

/// `printf("%.8g", (double)(float)v)`.
pub fn format_float(v: f64) -> String {
    let f = v as f32;
    if f == 0.0 {
        return "0".to_string();
    }
    format_g(f as f64, 8)
}

/// C `%.{prec}g` formatting.
pub fn format_g(v: f64, prec: usize) -> String {
    if v == 0.0 {
        return "0".to_string();
    }
    if !v.is_finite() {
        return if v.is_nan() { "nan".into() } else if v > 0.0 { "inf".into() } else { "-inf".into() };
    }
    let p = prec.max(1);
    let sci = format!("{:.*e}", p - 1, v);
    let (mant, exp) = sci.split_once('e').unwrap();
    let exp: i32 = exp.parse().unwrap();
    fn trim(s: &str) -> String {
        if s.contains('.') {
            s.trim_end_matches('0').trim_end_matches('.').to_string()
        } else {
            s.to_string()
        }
    }
    if exp < -4 || exp >= p as i32 {
        format!("{}e{}{:02}", trim(mant), if exp < 0 { '-' } else { '+' }, exp.abs())
    } else {
        let decimals = (p as i32 - 1 - exp).max(0) as usize;
        trim(&format!("{:.*}", decimals, v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn g_format_matches_c() {
        assert_eq!(format_float(1.0), "1");
        assert_eq!(format_float(0.0), "0");
        assert_eq!(format_float(1.0 / 3.0), "0.33333334");
        assert_eq!(format_float(0.083333336), "0.083333336");
        assert_eq!(format_float(1e-5), "9.9999997e-06");
        assert_eq!(format_float(0.5), "0.5");
        assert_eq!(format_float(123456789.0), "1.2345679e+08");
        assert_eq!(format_g(100.0, 8), "100");
        assert_eq!(format_g(0.0001, 8), "0.0001");
    }
}
