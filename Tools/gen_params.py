#!/usr/bin/env python3
"""Generates the parameter table and the Jup-8 V export template for the Rust crates (crates/core) and
the Swift reference implementation (swift/Sources/Jovian8Core).

Reads the parameter definitions shipped with an installed Arturia Jup-8 V4 (resources XML) and the
installed factory presets (for typical default values). Run `python3 -I Tools/xmlspec.py` first; it
writes Tools/spec.json. Nothing from Arturia is copied verbatim except parameter identifiers, value
ranges and list labels, which are needed for preset interoperability.
"""
import json, os, re, sys, collections, glob
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from robust import parse_params

ROOT = os.path.dirname(HERE)
spec = json.load(open(os.path.join(HERE, "spec.json")))
S, MAJOR = spec["specs"], spec["major"]
default_preset = dict(parse_params(open("/Library/Arturia/Presets/Jup-8 V4/Factory/Factory/Default", "rb").read())[1])

# ---- which parameters become plugin parameters, in address order (append-only!) ----
CORE = [
    # (id, group, short display name)
    ("LFO_RateUnSynced", "lfo", "LFO Rate"), ("LFO_RateSynced", "lfo", "LFO Rate (Sync)"), ("LFO_MidiSync", "lfo", "LFO Sync"),
    ("LFO_FadeDelay", "lfo", "LFO Delay"), ("LFO_Retrig", "lfo", "LFO Retrig"), ("LFO_Waveform", "lfo", "LFO Wave"),
    ("Osc1Coarse", "vco", "VCO-1 Range"), ("Osc1Wave", "vco", "VCO-1 Wave"), ("OscCrossMod", "vco", "Cross Mod"),
    ("Osc2Coarse", "vco", "VCO-2 Range"), ("Osc2Finetune", "vco", "VCO-2 Fine"), ("Osc2Wave", "vco", "VCO-2 Wave"),
    ("Osc2Mode", "vco", "VCO-2 Low Freq"), ("OscSyncSelect", "vco", "Sync"),
    ("OscLfoAmount", "vcomod", "VCO LFO Mod"), ("OscEnvAmount", "vcomod", "VCO Env-1 Mod"), ("OscDestSelect", "vcomod", "VCO Mod Dest"),
    ("OscPwmAmount", "vcomod", "Pulse Width"), ("OscPwmMode", "vcomod", "PWM Source"), ("OscMix", "vco", "Source Mix"),
    ("HPF_Cutoff", "vcf", "HPF Cutoff"), ("LpfCut", "vcf", "VCF Cutoff"), ("LpfRes", "vcf", "VCF Resonance"),
    ("LpfSlop", "vcf", "VCF Slope"), ("LpfEnvAmount", "vcf", "VCF Env Mod"), ("LpfEnvSource", "vcf", "VCF Env Select"),
    ("LfoFiltAmount", "vcf", "VCF LFO Mod"), ("LPF_KeyFollowAmount", "vcf", "VCF Key Follow"),
    ("VCA_EnvAmount", "vca", "VCA Level"), ("VCA_LfoAmount", "vca", "VCA LFO Mod"),
    ("Env1_Attack", "env1", "Env-1 Attack"), ("Env1_Decay", "env1", "Env-1 Decay"), ("Env1_Sustain", "env1", "Env-1 Sustain"),
    ("Env1_Release", "env1", "Env-1 Release"), ("Env1_KeyFollow", "env1", "Env-1 Key Follow"), ("Env1_Polarity", "env1", "Env-1 Invert"),
    ("Env2_Attack", "env2", "Env-2 Attack"), ("Env2_Decay", "env2", "Env-2 Decay"), ("Env2_Sustain", "env2", "Env-2 Sustain"),
    ("Env2Release", "env2", "Env-2 Release"), ("Env2_KeyFollow", "env2", "Env-2 Key Follow"),
    ("Volume", "master", "Volume"), ("MasterTune", "master", "Master Tune"), ("VoiceDetuneAmount", "voice", "Unison Detune"),
    ("PanSpread", "voice", "Pan Spread"), ("Portamento_Time", "voice", "Portamento"), ("Portamento_On", "voice", "Portamento On"),
    ("AssignMode", "voice", "Assign Mode"), ("PlPrio", "voice", "Note Priority"), ("UniVPN", "voice", "Unison Voices"),
    ("Polyphon", "voice", "Polyphony"),
    ("Condition", "dispersion", "Dispersion"), ("PitchDispersion", "dispersion", "Pitch Dispersion"),
    ("PWDispersion", "dispersion", "PW Dispersion"), ("EnvDispersion", "dispersion", "Env Dispersion"),
    ("CutoffDispersion", "dispersion", "Cutoff Dispersion"), ("ResoDispersion", "dispersion", "Reso Dispersion"),
    ("ModDispersion", "dispersion", "Mod Dispersion"),
    ("ArpSeq_ArpSeqMode", "arp", "Arp On"), ("ArpSeq_PatternMode", "arp", "Arp Mode"), ("ArpSeq_OctaveRepeat", "arp", "Arp Range"),
    ("ArpSeq_RateUnSynced", "arp", "Arp Rate"), ("ArpSeq_RateSynced", "arp", "Arp Rate (Sync)"), ("ArpSeq_MidiSync", "arp", "Arp Sync"),
    ("BendVCO1", "bender", "Bend VCO-1"), ("BendVCO2", "bender", "Bend VCO-2"), ("BendVCOAmount", "bender", "Bend VCO Range"),
    ("BendVCF", "bender", "Bend VCF"), ("BendVCFAmount", "bender", "Bend VCF Amount"),
    ("LfoModVCO", "modwheel", "Mod VCO"), ("LfoModVCOAmount", "modwheel", "Mod VCO Depth"),
    ("LfoModVCF", "modwheel", "Mod VCF"), ("LfoModVcfAmount", "modwheel", "Mod VCF Depth"),
]
LFO_PARAMS = ["Waveform", "RateMode", "RateUnSynced", "RateSynced", "Fade", "PhaseDist", "PhaseInit", "LoopMode", "Retrig", "Unipolar", "Poly"]
MATRIX_SLOTS = [(1, [1, 2]), (2, [3, 4]), (3, [5, 6, 7]), (4, [8, 9, 10]), (5, [11, 12, 13]), (6, [14, 15, 16]), (7, [17, 18, 19]), (8, [20, 21, 22]), (9, [23, 24])]
SRC_NAMES = {1: "LFO 2", 2: "LFO 3", 3: "Mod Mixer", 4: "Seq Mod", 5: "Velocity", 6: "Aftertouch", 7: "Keyboard", 8: "Mod Wheel", 9: "MPE Slide"}
MOD_DEST = ["None", "VCO1 Coarse", "VCO2 Coarse", "VCO1+2 Coarse", "VCO2 Fine tune", "VCO LFO Mod", "VCO Env1 Mod", "VCO1 PW", "VCO2 PW",
            "VCO PW Mod Amt", "VCO Cross Mod", "VCO Mix", "HPF Cutoff", "VCF Cutoff", "VCF Resonance", "VCF Env Mod Amt", "VCF LFO Mod Amt",
            "VCF Key F. Amt", "VCA Env 2 Amt", "VCA LFO Mod Amt", "Env1 Attack", "Env1 Decay", "Env1 Sustain", "Env1 Release", "Env2 Attack",
            "Env2 Decay", "Env2 Sustain", "Env2 Release", "Unison Detune", "Pan Spread", "Portamento", "LFO1 Rate", "LFO1 Delay", "LFO2 Rate",
            "LFO2 Phase", "LFO2 Warp", "LFO2 Fade", "LFO3 Rate", "LFO3 Phase", "LFO3 Warp", "LFO3 Fade", "ARP Rate", "Mod Mixer Amt", "Seq Rate",
            "Seq Mod Mult.", "Seq A. Decay", "Seq A. Amt", "Seq Glide", "FX1 Dry/Wet", "FX2 Dry/Wet", "FX3 Dry/Wet", "FX1 Param", "FX2 Param", "FX3 Param"]
assert len(MOD_DEST) == 54
KB_NAMES = {1: "Velocity", 2: "Aftertouch", 3: "Key Track", 4: "Mod Wheel"}
FX_SKIP = re.compile(r"InGainCompensated|ParamEq_Edited_Band")

entries = []  # (id, group, name, kind)
def add(pid, group, name, override=None):
    entries.append((pid, group, name, override))

for pid, g, n in CORE:
    add(pid, g, n)
for i, lname in ((1, "LFO 2"), (2, "LFO 3")):
    for p in LFO_PARAMS:
        disp = S[f"LFO{i}_{p}"]["display"].replace("Rate (Unsync)", "Rate").replace("Sync Mode", "Sync")
        add(f"LFO{i}_{p}", f"lfo{i+1}", f"{lname} {disp}")
for src, dests in MATRIX_SLOTS:
    for k, d in enumerate(dests):
        add(f"ModMatrixDest{d}", "matrix", f"{SRC_NAMES[src]} Dest {k+1}", ("discrete", MOD_DEST))
        add(f"ModMatrixAmount_Src{src}_Dest{d}", "matrix", f"{SRC_NAMES[src]} Amount {k+1}")
for kb in range(1, 5):
    for p in ["Y0", "X1", "Y1", "Y2"]:
        add(f"Keyboard{kb}_{p}", "keyboard", f"{KB_NAMES[kb]} Curve {p}")
add("FX_Enable", "fx", "FX On")
add("FX_Routing", "fx", "FX Routing")
fx_names = sorted(k for k in S if k.startswith("FX1_") and not FX_SKIP.search(k))
for slot in (1, 2, 3):
    for k in fx_names:
        pid = f"FX{slot}_" + k[4:]
        sub = k[4:]
        disp = S[k]["display"].strip()
        family = sub.split("_")[0] if "_" in sub else ""
        fam = {"Dattorro": "Reverb", "TS808": "Overdrive", "MultiModeFilter": "Filter", "ParamEq": "EQ", "StereoPan": "Pan"}.get(family, family)
        if fam and not disp.lower().startswith(fam.lower()):
            disp = f"{fam} {disp}"
        add(pid, f"fx{slot}", f"FX{slot} {disp}")

# ---- emit Swift ----
UNIT = {"Hz": "hz", "ms": "ms", "ms-s": "ms", "s": "seconds", "s-ms": "seconds", "s-ms-prec-2": "seconds", "st": "semitones",
        "%": "percent", "dB": "decibels", "°": "degrees", "bits": "bits", "x": "ratio", "bars": "none", "": "none", "BPM": "bpm"}
def curve(m):
    m = (m or "linear").strip()
    if m == "linear": return ".linear"
    if m == "reverse": return ".reverse"
    mm = re.match(r"(X|Exp|BipolarExp)\(([-0-9.]+)\)", m)
    assert mm, m
    kind, v = mm.group(1), float(mm.group(2))
    return {"X": f".power({v})", "Exp": f".exp({v})", "BipolarExp": f".bipolarExp({v})"}[kind]
def swift_str(s): return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'

lines = ["// Generated by Tools/gen_params.py from the installed Jup-8 V4 parameter definitions. Do not edit.",
         "// Addresses are the array index: the order is append-only so automation and saved states stay valid.", "",
         "extension ParamTable {", "    static let generated: [ParamSpec] = ["]
seen = set()
for pid, g, name, ov in entries:
    assert pid not in seen, pid
    seen.add(pid)
    s = S.get(pid) or S.get(re.sub(r"^FX[23]_", "FX1_", pid))
    if ov:
        kind = "        .discrete([" + ", ".join(swift_str(x) for x in ov[1]) + "])"
    elif s and s["items"]:
        kind = "        .discrete([" + ", ".join(swift_str(x) for x in s["items"]) + "])"
    else:
        lo = float(s["min"]) if s and s["min"] is not None else 0.0
        hi = float(s["max"]) if s and s["max"] is not None else 1.0
        kind = f"        .continuous({curve(s['mapping'] if s else None)}, min: {lo}, max: {hi}, unit: .{UNIT.get((s or {}).get('unit', ''), 'none')})"
    dv = default_preset.get(pid, MAJOR.get(pid))
    if dv is None:
        dv = float(s["defnorm"]) if s and s.get("defnorm") else 0.0
    lines.append(f"        ParamSpec(id: {swift_str(pid)}, name: {swift_str(name)}, group: .{g}, defaultValue: {float(dv)!r},")
    lines.append(kind + "),")
lines += ["    ]", "}", ""]
open(os.path.join(ROOT, "swift/Sources/Jovian8Core/Params/ParamTable+Generated.swift"), "w").write("\n".join(lines))

# ---- export template: full modern parameter set with neutral values ----
files = sorted(glob.glob("/Library/Arturia/Presets/Jup-8 V4/Factory/*/*"))
count = collections.Counter()
for f in files:
    for k, _ in parse_params(open(f, "rb").read())[1]:
        count[k] += 1
names = sorted(set(default_preset) | {k for k, c in count.items() if c >= 300})
tmpl = []
for k in names:
    v = default_preset.get(k, MAJOR.get(k, 0.0))
    tmpl.append(f"{k}\t{float(v)!r}")
out = ["// Generated by Tools/gen_params.py: the full Jup-8 V4 parameter set with neutral values, used as the",
       "// base when writing presets so that parameters this plugin does not model get sane values. Do not edit.", "",
       "enum ArturiaTemplate {", '    static let text = """'] + tmpl + ['"""', "}", ""]
open(os.path.join(ROOT, "swift/Sources/Jovian8Core/Preset/ArturiaTemplate.swift"), "w").write("\n".join(out))
print(f"{len(entries)} plugin parameters, {len(tmpl)} template parameters")

# ---- emit Rust (crates/core) ----
def rust_curve(m):
    m = (m or "linear").strip()
    if m == "linear": return "Curve::Linear"
    if m == "reverse": return "Curve::Reverse"
    mm = re.match(r"(X|Exp|BipolarExp)\(([-0-9.]+)\)", m)
    kind, v = mm.group(1), float(mm.group(2))
    return {"X": f"Curve::Power({v!r})", "Exp": f"Curve::Exp({v!r})", "BipolarExp": f"Curve::BipolarExp({v!r})"}[kind]
def rust_str(s): return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'
RGROUP = {g: "".join(w.capitalize() for w in re.findall(r"[a-z]+|\d+", g)) for g in
          ["lfo", "vco", "vcomod", "vcf", "vca", "env1", "env2", "master", "voice", "dispersion", "arp", "bender", "modwheel",
           "lfo2", "lfo3", "matrix", "keyboard", "fx", "fx1", "fx2", "fx3"]}
RGROUP.update({"vcomod": "VcoMod", "modwheel": "ModWheel", "env1": "Env1", "env2": "Env2", "lfo2": "Lfo2", "lfo3": "Lfo3",
               "fx1": "Fx1", "fx2": "Fx2", "fx3": "Fx3"})
RUNIT = {"hz": "Hz", "ms": "Ms", "seconds": "Seconds", "semitones": "Semitones", "percent": "Percent", "decibels": "Decibels",
         "degrees": "Degrees", "bits": "Bits", "ratio": "Ratio", "bpm": "Bpm", "none": "None"}
rl = ["// Generated by Tools/gen_params.py from the installed Jup-8 V4 parameter definitions. Do not edit.",
      "// Addresses are the array index: the order is append-only so automation and saved states stay valid.", "",
      "use super::{Curve, Group, Kind, ParamSpec, Unit};", "",
      "pub static PARAMS: &[ParamSpec] = &["]
consts = []
for i, (pid, g, name, ov) in enumerate(entries):
    s = S.get(pid) or S.get(re.sub(r"^FX[23]_", "FX1_", pid))
    if ov:
        kind = "Kind::Discrete(&[" + ", ".join(rust_str(x) for x in ov[1]) + "])"
    elif s and s["items"]:
        kind = "Kind::Discrete(&[" + ", ".join(rust_str(x) for x in s["items"]) + "])"
    else:
        lo = float(s["min"]) if s and s["min"] is not None else 0.0
        hi = float(s["max"]) if s and s["max"] is not None else 1.0
        unit = RUNIT[UNIT.get((s or {}).get('unit', ''), 'none')]
        kind = f"Kind::Continuous {{ curve: {rust_curve(s['mapping'] if s else None)}, min: {lo!r}, max: {hi!r}, unit: Unit::{unit} }}"
    dv = default_preset.get(pid, MAJOR.get(pid))
    if dv is None:
        dv = float(s["defnorm"]) if s and s.get("defnorm") else 0.0
    rl.append(f"    ParamSpec {{ id: {rust_str(pid)}, name: {rust_str(name)}, group: Group::{RGROUP[g]}, default: {float(dv)!r}, kind: {kind} }},")
    consts.append(f"    pub const {pid}: usize = {i};")
rl += ["];", "", "/// Parameter addresses by Jup-8 V id.", "#[allow(non_upper_case_globals)]", "pub mod addr {"] + consts + ["}", ""]
rdir = os.path.join(ROOT, "crates/core/src")
os.makedirs(os.path.join(rdir, "params"), exist_ok=True)
os.makedirs(os.path.join(rdir, "preset"), exist_ok=True)
open(os.path.join(rdir, "params/table.rs"), "w").write("\n".join(rl))
open(os.path.join(rdir, "preset/arturia_template.tsv"), "w").write("\n".join(tmpl) + "\n")
print("wrote Rust table and template")
