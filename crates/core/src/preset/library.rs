//! Preset files on disk: Jup-8 V's own library and the user's Jovian-8 folder.

use super::arturia::{ArturiaPreset, ParseError};
use super::jup4x;
use super::state::PatchState;
use std::path::{Path, PathBuf};

/// Where Jup-8 V keeps its factory and user presets.
pub const ARTURIA_ROOT: &str = "/Library/Arturia/Presets/Jup-8 V4";

/// The user's home folder.
pub fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/tmp"))
}

/// ~/Music/Jovian-8: exports go to Exports/, and any .jup4x or preset file dropped anywhere in
/// this folder shows up in the browser.
pub fn user_folder() -> PathBuf {
    home().join("Music/Jovian-8")
}

pub fn export_folder() -> PathBuf {
    user_folder().join("Exports")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Source {
    /// Patches compiled into the plugin.
    BuiltIn,
    ArturiaFactory,
    ArturiaUser,
    /// Files in ~/Music/Jovian-8.
    User,
}

impl Source {
    pub fn title(self) -> &'static str {
        match self {
            Source::BuiltIn => "Jovian-8",
            Source::ArturiaFactory => "Jup-8 V Factory",
            Source::ArturiaUser => "Jup-8 V User",
            Source::User => "My Presets",
        }
    }
}

/// A preset found on disk (or built in).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LibraryEntry {
    pub name: String,
    pub bank: String,
    pub source: Source,
    /// Empty for built-in patches.
    pub path: PathBuf,
    /// Index inside a multi-preset archive, or of the built-in patch.
    pub index: usize,
    /// Jup-8 V browser type ("Bass", "Pad", ...) when known.
    pub category: String,
}

impl LibraryEntry {
    pub fn load(&self) -> Result<PatchState, Box<dyn std::error::Error + Send + Sync>> {
        if self.source == Source::BuiltIn {
            return super::factory::all().into_iter().nth(self.index).ok_or_else(|| "no such built-in patch".into());
        }
        let data = std::fs::read(&self.path)?;
        let presets = jup4x::read(&data)?;
        let p = presets.get(self.index).ok_or(ParseError::NoParameterMap)?;
        Ok(PatchState::from_preset(p))
    }

    /// Stable key for remembering a selection.
    pub fn key(&self) -> String {
        format!("{}#{}", if self.path.as_os_str().is_empty() { "builtin".into() } else { self.path.display().to_string() }, self.index)
    }
}

/// The file name has a real extension ("x.jup4x"), not just a dot in a preset name ("T. Sawyer").
pub fn real_extension(p: &Path) -> Option<String> {
    let e = p.extension()?.to_string_lossy().to_lowercase();
    (!e.is_empty() && e.len() <= 8 && e.chars().all(|c| c.is_ascii_alphanumeric())).then_some(e)
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut items: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
    items.sort();
    for p in items {
        if p.is_dir() {
            walk(&p, out);
        } else if p.is_file() {
            out.push(p);
        }
    }
}

/// Reads only the browser type of a preset without parsing its parameters.
fn quick_category(data: &[u8]) -> String {
    ArturiaPreset::parse(data, false).ok().and_then(|p| p.meta.get("Type").cloned()).unwrap_or_default()
}

/// Lists the built-in patches, installed Jup-8 V presets and the user's own folder.
/// Unreadable locations are skipped.
pub fn scan() -> Vec<LibraryEntry> {
    let mut out: Vec<LibraryEntry> = super::factory::all()
        .iter()
        .enumerate()
        .map(|(i, p)| LibraryEntry {
            name: p.name.clone(),
            bank: "Jovian-8".into(),
            source: Source::BuiltIn,
            path: PathBuf::new(),
            index: i,
            category: String::new(),
        })
        .collect();
    let root = Path::new(ARTURIA_ROOT);
    let mut files = Vec::new();
    walk(root, &mut files);
    for f in files {
        if real_extension(&f).is_some() {
            continue;
        }
        let Ok(rel) = f.strip_prefix(root) else { continue };
        let parts: Vec<String> = rel.iter().map(|s| s.to_string_lossy().into_owned()).collect();
        if parts.len() < 2 || parts[0] == "Tutorials" {
            continue;
        }
        let Ok(data) = std::fs::read(&f) else { continue };
        if !ArturiaPreset::looks_like_preset(&data) {
            continue;
        }
        out.push(LibraryEntry {
            name: f.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            bank: parts[parts.len() - 2].clone(),
            source: if parts[0] == "Factory" { Source::ArturiaFactory } else { Source::ArturiaUser },
            path: f.clone(),
            index: 0,
            category: quick_category(&data),
        });
    }
    let mut files = Vec::new();
    walk(&user_folder(), &mut files);
    for f in files {
        let ext = real_extension(&f).unwrap_or_default();
        if ext == jup4x::FILE_EXTENSION {
            let Ok(data) = std::fs::read(&f) else { continue };
            let Ok(presets) = jup4x::read(&data) else { continue };
            for (i, p) in presets.iter().enumerate() {
                out.push(LibraryEntry {
                    name: p.name.clone(),
                    bank: p.pack.clone(),
                    source: Source::User,
                    path: f.clone(),
                    index: i,
                    category: p.meta.get("Type").cloned().unwrap_or_default(),
                });
            }
        } else if ext.is_empty() {
            let Ok(data) = std::fs::read(&f) else { continue };
            if !ArturiaPreset::looks_like_preset(&data) {
                continue;
            }
            out.push(LibraryEntry {
                name: f.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                bank: f.parent().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                source: Source::User,
                path: f.clone(),
                index: 0,
                category: quick_category(&data),
            });
        }
    }
    out.sort_by(|a, b| (a.source, &a.bank, &a.name, a.index).cmp(&(b.source, &b.bank, &b.name, b.index)));
    out
}

/// Writes a `.jup4x` for Jup-8 V's Import command; returns the file path.
pub fn export(states: &[PatchState], file_name: &str, folder: Option<&Path>) -> std::io::Result<PathBuf> {
    let folder = folder.map(Path::to_path_buf).unwrap_or_else(export_folder);
    std::fs::create_dir_all(&folder)?;
    let path = folder.join(format!("{}.{}", jup4x::sanitize(file_name), jup4x::FILE_EXTENSION));
    let presets: Vec<ArturiaPreset> = states.iter().map(|s| s.arturia_preset()).collect();
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, jup4x::make(&presets))?;
    std::fs::rename(&tmp, &path)?;
    Ok(path)
}

/// Copies an imported archive or preset file into the user folder so it appears in the browser.
pub fn import_file(src: &Path) -> std::io::Result<PathBuf> {
    let dir = user_folder();
    std::fs::create_dir_all(&dir)?;
    let dest = dir.join(src.file_name().unwrap_or_default());
    if dest != src {
        std::fs::copy(src, &dest)?;
    }
    Ok(dest)
}
