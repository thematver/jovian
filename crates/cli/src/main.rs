//! Command line companion: render presets offline, inspect and convert Jup-8 V preset files.

use jovian_core::params::{self, Group, PARAMS};
use jovian_core::preset::{jup4x, library, ArturiaPreset, PatchState};
use jovian_core::{HostTime, SynthEngine};
use std::path::Path;
use std::process::exit;

const USAGE: &str = "usage:
  jovian8 info <preset|.jup4x>                       show a preset's header and modelled values
  jovian8 render <preset|.jup4x|-> <out.wav> [script]   render with the Jovian-8 engine ('-' = Init)
        script: lines \"on <note> <vel> <sec>\", \"off <note> <sec>\", \"cc <n> <v> <sec>\", \"bend <v14> <sec>\",
                \"len <sec>\", \"set <ParamID>=<normalized>\"; default plays C4 for 1 s
  jovian8 export <out.jup4x> <preset|.jup4x>...       bundle presets into a Jup-8 V import file
  jovian8 export-builtin <out.jup4x>                   export the built-in Jovian-8 patches
  jovian8 roundtrip [folder]                           check parse -> state -> preset -> parse on a library
  jovian8 list                                         list presets found by the plugin browser
  jovian8 compare <a.wav> <b.wav>                      peak / RMS difference of two renders";

fn fail(s: &str) -> ! {
    eprintln!("{s}");
    exit(1)
}

fn load_states(path: &str) -> Vec<PatchState> {
    if path == "-" {
        return vec![PatchState::default()];
    }
    let data = std::fs::read(path).unwrap_or_else(|e| fail(&format!("cannot read {path}: {e}")));
    match jup4x::read(&data) {
        Ok(v) => v.iter().map(PatchState::from_preset).collect(),
        Err(e) => fail(&format!("{path}: {e}")),
    }
}

fn write_wav(path: &str, l: &[f32], r: &[f32], sample_rate: u32) -> std::io::Result<()> {
    let mut d = Vec::with_capacity(44 + l.len() * 8);
    let bytes = (l.len() * 8) as u32;
    d.extend_from_slice(b"RIFF");
    d.extend_from_slice(&(36 + bytes).to_le_bytes());
    d.extend_from_slice(b"WAVEfmt ");
    d.extend_from_slice(&16u32.to_le_bytes());
    d.extend_from_slice(&3u16.to_le_bytes());
    d.extend_from_slice(&2u16.to_le_bytes());
    d.extend_from_slice(&sample_rate.to_le_bytes());
    d.extend_from_slice(&(sample_rate * 8).to_le_bytes());
    d.extend_from_slice(&8u16.to_le_bytes());
    d.extend_from_slice(&32u16.to_le_bytes());
    d.extend_from_slice(b"data");
    d.extend_from_slice(&bytes.to_le_bytes());
    for i in 0..l.len() {
        d.extend_from_slice(&l[i].to_le_bytes());
        d.extend_from_slice(&r[i].to_le_bytes());
    }
    std::fs::write(path, d)
}

/// Reads a 32-bit float stereo WAV as written by `write_wav` (or the Swift CLI).
fn read_wav(path: &str) -> (Vec<f32>, Vec<f32>) {
    let d = std::fs::read(path).unwrap_or_else(|e| fail(&format!("cannot read {path}: {e}")));
    let mut p = 12;
    while p + 8 <= d.len() {
        let id = &d[p..p + 4];
        let size = u32::from_le_bytes([d[p + 4], d[p + 5], d[p + 6], d[p + 7]]) as usize;
        if id == b"data" {
            let s = &d[p + 8..(p + 8 + size).min(d.len())];
            let (mut l, mut r) = (Vec::new(), Vec::new());
            for f in s.chunks_exact(8) {
                l.push(f32::from_le_bytes([f[0], f[1], f[2], f[3]]));
                r.push(f32::from_le_bytes([f[4], f[5], f[6], f[7]]));
            }
            return (l, r);
        }
        p += 8 + size;
    }
    fail(&format!("{path}: no data chunk"))
}

struct Ev {
    frame: usize,
    bytes: (u8, u8, u8),
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = args.first() else {
        println!("{USAGE}");
        return;
    };
    match cmd.as_str() {
        "info" => {
            let path = args.get(1).unwrap_or_else(|| fail(USAGE));
            let data = std::fs::read(path).unwrap_or_else(|e| fail(&e.to_string()));
            for p in jup4x::read(&data).unwrap_or_else(|e| fail(&e.to_string())) {
                println!(
                    "{}  [bank {}]  by {}  v{}  {} params, {} blobs",
                    p.name, p.pack, p.author, p.version, p.params.len(), p.blobs.len()
                );
                if !p.comment.is_empty() {
                    println!("  {}", p.comment);
                }
                for (k, v) in &p.meta {
                    println!("  meta {k} = {v}");
                }
                let st = PatchState::from_preset(&p);
                for s in PARAMS {
                    if matches!(s.group, Group::Fx1 | Group::Fx2 | Group::Fx3 | Group::Matrix | Group::Keyboard) {
                        continue;
                    }
                    println!("  {:<22} {:<24} {}", s.id, s.name, s.display_string(st.value(s.id)));
                }
            }
        }
        "render" => {
            if args.len() < 3 {
                fail(USAGE)
            }
            let state = load_states(&args[1]).into_iter().next().unwrap_or_default();
            let mut engine = SynthEngine::new();
            let sr = 48_000usize;
            engine.prepare(sr as f64, 512);
            engine.params = state.to_values();
            let mut script = "on 60 100 0.0\noff 60 1.0\nlen 2.5".to_string();
            if let Some(s) = args.get(3).and_then(|p| std::fs::read_to_string(p).ok()) {
                script = s;
            }
            let mut events = Vec::new();
            let mut length = 2.5;
            for raw in script.lines() {
                let f: Vec<&str> = raw.split(' ').filter(|s| !s.is_empty()).collect();
                let Some(c) = f.first() else { continue };
                let t = |i: usize| (f.get(i).and_then(|x| x.parse::<f64>().ok()).unwrap_or(0.0) * sr as f64) as usize;
                let num = |i: usize| f.get(i).and_then(|x| x.parse::<i64>().ok()).unwrap_or(0);
                match *c {
                    "on" => events.push(Ev { frame: t(3), bytes: (0x90, num(1) as u8, num(2) as u8) }),
                    "off" => events.push(Ev { frame: t(2), bytes: (0x80, num(1) as u8, 0) }),
                    "cc" => events.push(Ev { frame: t(3), bytes: (0xB0, num(1) as u8, num(2) as u8) }),
                    "bend" => {
                        let v = num(1);
                        events.push(Ev { frame: t(2), bytes: (0xE0, (v & 0x7F) as u8, (v >> 7 & 0x7F) as u8) })
                    }
                    "len" => length = f.get(1).and_then(|x| x.parse().ok()).unwrap_or(length),
                    "set" => {
                        if let Some((k, v)) = f.get(1).and_then(|s| s.split_once('=')) {
                            if let Some(a) = params::address(k) {
                                engine.params[a] = v.parse().unwrap_or(0.0);
                            }
                        }
                    }
                    _ => {}
                }
            }
            events.sort_by_key(|e| e.frame);
            let total = (length * sr as f64) as usize;
            let (mut out_l, mut out_r) = (vec![0f32; total], vec![0f32; total]);
            let block = 256;
            let (mut ei, mut pos) = (0, 0);
            let (mut bl, mut br) = (vec![0f32; block], vec![0f32; block]);
            let start = std::time::Instant::now();
            while pos < total {
                let mut n = block.min(total - pos);
                if ei < events.len() && events[ei].frame <= pos {
                    let b = events[ei].bytes;
                    engine.handle_midi(b.0, b.1, b.2);
                    ei += 1;
                    continue;
                }
                if ei < events.len() {
                    n = n.min(events[ei].frame - pos);
                }
                let time = HostTime { tempo: 120.0, beat_position: pos as f64 / sr as f64 * 2.0, is_playing: true };
                engine.render(&mut bl[..n], &mut br[..n], time);
                out_l[pos..pos + n].copy_from_slice(&bl[..n]);
                out_r[pos..pos + n].copy_from_slice(&br[..n]);
                pos += n;
            }
            let secs = start.elapsed().as_secs_f64();
            write_wav(&args[2], &out_l, &out_r, sr as u32).unwrap_or_else(|e| fail(&e.to_string()));
            let peak = out_l.iter().fold(0f32, |m, x| m.max(x.abs()));
            println!("rendered {} -> {}  peak {:.4}  realtime x{:.1}", state.name, args[2], peak, length / secs.max(1e-6));
        }
        "export" => {
            if args.len() < 3 {
                fail(USAGE)
            }
            let states: Vec<PatchState> = args[2..].iter().flat_map(|p| load_states(p)).collect();
            if states.is_empty() {
                fail("no presets to export")
            }
            let presets: Vec<ArturiaPreset> = states.iter().map(|s| s.arturia_preset()).collect();
            std::fs::write(&args[1], jup4x::make(&presets)).unwrap_or_else(|e| fail(&e.to_string()));
            println!("wrote {} presets to {}", states.len(), args[1]);
        }
        "export-builtin" => {
            let out = args.get(1).unwrap_or_else(|| fail(USAGE));
            let states = jovian_core::preset::factory::all();
            let presets: Vec<ArturiaPreset> = states.iter().map(|s| s.arturia_preset()).collect();
            std::fs::write(out, jup4x::make(&presets)).unwrap_or_else(|e| fail(&e.to_string()));
            println!("wrote {} presets to {}", states.len(), out);
        }
        "roundtrip" => {
            let root = args.get(1).cloned().unwrap_or_else(|| library::ARTURIA_ROOT.to_string());
            let (mut ok, mut failed, mut identical) = (0, 0, 0);
            let mut max_err = 0f64;
            let mut files = Vec::new();
            collect(Path::new(&root), &mut files);
            for f in files {
                if library::real_extension(&f).is_some() {
                    continue;
                }
                let Ok(data) = std::fs::read(&f) else { continue };
                if !ArturiaPreset::looks_like_preset(&data) {
                    continue;
                }
                let res = (|| -> Result<(), String> {
                    let original = ArturiaPreset::parse(&data, false).map_err(|e| e.to_string())?;
                    if original.serialized() == data {
                        identical += 1;
                    }
                    let state = PatchState::from_preset(&original);
                    // the live path: values go through the f32 parameter array and back
                    let mut live_state = state.clone();
                    live_state.capture(&state.to_values());
                    if live_state != state {
                        return Err("capture changed values".into());
                    }
                    let written = state.arturia_preset().serialized();
                    let reread = ArturiaPreset::parse(&written, false).map_err(|e| e.to_string())?;
                    for (k, v) in &original.params {
                        let w = reread.params.get(k).ok_or(format!("missing {k}"))?;
                        let expected = if params::address(k).is_some() {
                            original.normalized_for_current_version(k).unwrap_or(*v)
                        } else {
                            *v
                        };
                        max_err = max_err.max(((expected as f32) as f64 - w).abs());
                    }
                    if reread.name != original.name {
                        return Err("header".into());
                    }
                    let state2 = PatchState::from_preset(&reread);
                    if state2.values != state.values {
                        return Err("values differ".into());
                    }
                    Ok(())
                })();
                match res {
                    Ok(()) => ok += 1,
                    Err(e) => {
                        failed += 1;
                        println!("FAIL {}: {e}", f.display());
                    }
                }
            }
            println!("round trip: {ok} ok, {failed} failed, max value error {max_err}; re-serialized byte-identical: {identical}");
        }
        "list" => {
            for e in library::scan() {
                println!("{} / {} / {} {}", e.source.title(), e.bank, e.name, if e.category.is_empty() { String::new() } else { format!("({})", e.category) });
            }
        }
        "compare" => {
            if args.len() < 3 {
                fail(USAGE)
            }
            let (al, ar) = read_wav(&args[1]);
            let (bl, br) = read_wav(&args[2]);
            let n = al.len().min(bl.len());
            let (mut peak, mut diff2, mut sig2) = (0f64, 0f64, 0f64);
            let mut first = None;
            for i in 0..n {
                for (x, y) in [(al[i], bl[i]), (ar[i], br[i])] {
                    let d = (x - y).abs() as f64;
                    if d > 1e-6 && first.is_none() {
                        first = Some(i);
                    }
                    peak = peak.max(d);
                    diff2 += d * d;
                    sig2 += (x as f64) * (x as f64);
                }
            }
            let db = |x: f64| 10.0 * x.max(1e-30).log10();
            println!(
                "samples {n} (lengths {} / {})  peak diff {peak:.3e}  diff/signal {:.1} dB  first diff > 1e-6 at {:?}",
                al.len(),
                bl.len(),
                db(diff2 / sig2.max(1e-30)),
                first
            );
        }
        _ => println!("{USAGE}"),
    }
}

fn collect(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut items: Vec<_> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
    items.sort();
    for p in items {
        if p.is_dir() {
            collect(&p, out)
        } else {
            out.push(p)
        }
    }
}
