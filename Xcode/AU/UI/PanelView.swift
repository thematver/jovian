import Jovian8Core
import SwiftUI

struct RootView: View {
    @ObservedObject var model: ParamModel
    @State private var tab: Tab

    init(model: ParamModel, initialTab: Tab = .panel) {
        self.model = model
        _tab = State(initialValue: initialTab)
    }

    enum Tab: String, CaseIterable { case panel = "Panel", modulation = "Modulation", effects = "Effects", presets = "Presets" }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 14) {
                Text("JOVIAN-8").font(.system(size: 17, weight: .heavy)).tracking(4).foregroundStyle(Palette.text)
                Text("polyphonic synthesizer").font(.system(size: 10)).foregroundStyle(Palette.dim)
                Spacer()
                Text(model.patchName).font(.system(size: 12, weight: .semibold)).foregroundStyle(Palette.yellow)
                    .padding(.horizontal, 10).padding(.vertical, 3)
                    .background(RoundedRectangle(cornerRadius: 3).fill(Color.black))
                Picker("", selection: $tab) {
                    ForEach(Tab.allCases, id: \.self) { Text($0.rawValue).tag($0) }
                }
                .pickerStyle(.segmented).labelsHidden().frame(width: 330)
            }
            .padding(.horizontal, 14).padding(.vertical, 8)
            .background(Color.black.opacity(0.6))
            Rectangle().fill(LinearGradient(colors: [Palette.red, Palette.orange, Palette.yellow, Palette.blue], startPoint: .leading, endPoint: .trailing)).frame(height: 3)
            Group {
                switch tab {
                case .panel: PanelView(m: model)
                case .modulation: ModulationView(m: model)
                case .effects: EffectsView(m: model)
                case .presets: PresetBrowserView(model: model)
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        }
        .background(Palette.panel)
        .preferredColorScheme(.dark)
    }
}

/// The front panel, laid out like the Jupiter-8 (upper row: sound sources and filters,
/// lower row: envelopes and performance controls).
struct PanelView: View {
    let m: ParamModel

    var body: some View {
        ScrollView([.horizontal, .vertical]) {
            VStack(alignment: .leading, spacing: 8) {
                HStack(alignment: .top, spacing: 2) {
                    PanelSection(title: "LFO") {
                        Fader(cell: m["LFO_RateUnSynced"], label: "RATE")
                        Fader(cell: m["LFO_FadeDelay"], label: "DELAY")
                        VStack(spacing: 3) {
                            Selector(cell: m["LFO_Waveform"], labels: ["SINE", "SAW", "SQR", "RND"], color: Palette.orange)
                            Toggle2(cell: m["LFO_Retrig"], title: "RETRIG", color: Palette.white)
                            Toggle2(cell: m["LFO_MidiSync"], title: "SYNC", color: Palette.white)
                        }
                    }
                    PanelSection(title: "VCO Modulator") {
                        Fader(cell: m["OscLfoAmount"], label: "LFO MOD")
                        Fader(cell: m["OscEnvAmount"], label: "ENV-1")
                        Selector(cell: m["OscDestSelect"], labels: ["VCO-2", "1 & 2", "VCO-1"], color: Palette.yellow)
                    }
                    PanelSection(title: "Pulse Width") {
                        Fader(cell: m["OscPwmAmount"], label: "PW/PWM")
                        Selector(cell: m["OscPwmMode"], labels: ["ENV-1", "MAN", "LFO"], color: Palette.yellow)
                    }
                    PanelSection(title: "VCO-1") {
                        Fader(cell: m["OscCrossMod"], label: "X-MOD")
                        Selector(cell: m["Osc1Coarse"], color: Palette.orange)
                        Selector(cell: m["Osc1Wave"], labels: ["TRI", "SAW", "PULSE", "SQR"], color: Palette.orange)
                    }
                    PanelSection(title: "VCO-2") {
                        VStack(spacing: 3) {
                            Selector(cell: m["OscSyncSelect"], labels: ["SYNC 2→1", "OFF", "SYNC 1→2"], color: Palette.white)
                            Toggle2(cell: m["Osc2Mode"], title: "LOW FREQ", color: Palette.white)
                        }
                        Fader(cell: m["Osc2Coarse"], label: "RANGE")
                        Fader(cell: m["Osc2Finetune"], label: "FINE")
                        Selector(cell: m["Osc2Wave"], labels: ["SINE", "SAW", "PULSE", "NOISE"], color: Palette.orange)
                    }
                    PanelSection(title: "Mixer") {
                        Fader(cell: m["OscMix"], label: "SOURCE MIX")
                        Fader(cell: m["HPF_Cutoff"], label: "HPF")
                    }
                }
                HStack(alignment: .top, spacing: 2) {
                    PanelSection(title: "VCF") {
                        Fader(cell: m["LpfCut"], label: "FREQ")
                        Fader(cell: m["LpfRes"], label: "RES")
                        VStack(spacing: 3) {
                            Selector(cell: m["LpfSlop"], labels: ["-12dB", "-24dB"], color: Palette.blue)
                            Selector(cell: m["LpfEnvSource"], labels: ["ENV-1", "ENV-2"], color: Palette.blue)
                        }
                        Fader(cell: m["LpfEnvAmount"], label: "ENV MOD")
                        Fader(cell: m["LfoFiltAmount"], label: "LFO MOD")
                        Fader(cell: m["LPF_KeyFollowAmount"], label: "KYBD")
                    }
                    PanelSection(title: "VCA") {
                        Fader(cell: m["VCA_EnvAmount"], label: "LEVEL")
                        Fader(cell: m["VCA_LfoAmount"], label: "LFO MOD")
                    }
                    PanelSection(title: "ENV-1") {
                        Fader(cell: m["Env1_Attack"], label: "A")
                        Fader(cell: m["Env1_Decay"], label: "D")
                        Fader(cell: m["Env1_Sustain"], label: "S")
                        Fader(cell: m["Env1_Release"], label: "R")
                        VStack(spacing: 3) {
                            Toggle2(cell: m["Env1_KeyFollow"], title: "KEY FLW", color: Palette.white)
                            Toggle2(cell: m["Env1_Polarity"], title: "INVERT", color: Palette.white)
                        }
                    }
                    PanelSection(title: "ENV-2") {
                        Fader(cell: m["Env2_Attack"], label: "A")
                        Fader(cell: m["Env2_Decay"], label: "D")
                        Fader(cell: m["Env2_Sustain"], label: "S")
                        Fader(cell: m["Env2Release"], label: "R")
                        Toggle2(cell: m["Env2_KeyFollow"], title: "KEY FLW", color: Palette.white)
                    }
                }
                HStack(alignment: .top, spacing: 2) {
                    PanelSection(title: "Assign") {
                        Selector(cell: m["AssignMode"], labels: ["SOLO", "UNISON", "POLY 1", "POLY 2"], color: Palette.red)
                        VStack(alignment: .leading, spacing: 4) {
                            ParamRow(cell: m["Polyphon"], label: "Voices", width: 128)
                            ParamRow(cell: m["UniVPN"], label: "Unison", width: 128)
                            ParamRow(cell: m["PlPrio"], label: "Priority", width: 128)
                        }
                        Fader(cell: m["VoiceDetuneAmount"], label: "DETUNE", height: 70)
                        Fader(cell: m["PanSpread"], label: "SPREAD", height: 70)
                    }
                    PanelSection(title: "Arpeggio") {
                        VStack(spacing: 3) {
                            Toggle2(cell: m["ArpSeq_ArpSeqMode"], title: "ARP ON", color: Palette.red)
                            Toggle2(cell: m["ArpSeq_MidiSync"], title: "SYNC", color: Palette.white)
                        }
                        Selector(cell: m["ArpSeq_PatternMode"], color: Palette.red)
                        Selector(cell: m["ArpSeq_OctaveRepeat"], labels: ["1 OCT", "2 OCT", "3 OCT", "4 OCT"], color: Palette.red)
                        VStack(alignment: .leading, spacing: 4) {
                            Fader(cell: m["ArpSeq_RateUnSynced"], label: "RATE", height: 70)
                            ParamRow(cell: m["ArpSeq_RateSynced"], label: "Sync", width: 120)
                        }
                    }
                    PanelSection(title: "Bender / Mod") {
                        VStack(spacing: 3) {
                            Toggle2(cell: m["BendVCO1"], title: "BEND VCO-1", color: Palette.white)
                            Toggle2(cell: m["BendVCO2"], title: "BEND VCO-2", color: Palette.white)
                            Toggle2(cell: m["BendVCF"], title: "BEND VCF", color: Palette.white)
                        }
                        Fader(cell: m["BendVCOAmount"], label: "VCO", height: 70)
                        Fader(cell: m["BendVCFAmount"], label: "VCF", height: 70)
                        VStack(spacing: 3) {
                            Toggle2(cell: m["LfoModVCO"], title: "MOD VCO", color: Palette.white)
                            Toggle2(cell: m["LfoModVCF"], title: "MOD VCF", color: Palette.white)
                        }
                        Fader(cell: m["LfoModVCOAmount"], label: "VCO", height: 70)
                        Fader(cell: m["LfoModVcfAmount"], label: "VCF", height: 70)
                    }
                    PanelSection(title: "Master") {
                        VStack(spacing: 3) {
                            Toggle2(cell: m["Portamento_On"], title: "PORTA", color: Palette.white)
                        }
                        Fader(cell: m["Portamento_Time"], label: "TIME", height: 70)
                        Fader(cell: m["MasterTune"], label: "TUNE", height: 70)
                        Fader(cell: m["Volume"], label: "VOLUME", height: 70)
                    }
                    PanelSection(title: "Dispersion") {
                        VStack(alignment: .leading, spacing: 3) {
                            ParamRow(cell: m["PitchDispersion"], label: "Pitch", width: 132)
                            ParamRow(cell: m["PWDispersion"], label: "Pulse width", width: 132)
                            ParamRow(cell: m["EnvDispersion"], label: "Envelopes", width: 132)
                            ParamRow(cell: m["CutoffDispersion"], label: "Cutoff", width: 132)
                            ParamRow(cell: m["ResoDispersion"], label: "Resonance", width: 132)
                            ParamRow(cell: m["ModDispersion"], label: "Modulation", width: 132)
                        }
                    }
                }
            }
            .padding(10)
        }
    }
}

struct ModulationView: View {
    let m: ParamModel
    static let sources: [(String, [Int])] = [("LFO 2", [1, 2]), ("LFO 3", [3, 4]), ("Velocity", [11, 12, 13]), ("Aftertouch", [14, 15, 16]),
                                             ("Keyboard", [17, 18, 19]), ("Mod Wheel", [20, 21, 22]), ("MPE Slide", [23, 24]),
                                             ("Mod Mixer", [5, 6, 7]), ("Mod Seq", [8, 9, 10])]
    static let srcIndex = ["LFO 2": 1, "LFO 3": 2, "Mod Mixer": 3, "Mod Seq": 4, "Velocity": 5, "Aftertouch": 6, "Keyboard": 7, "Mod Wheel": 8, "MPE Slide": 9]

    var body: some View {
        ScrollView {
            HStack(alignment: .top, spacing: 16) {
                VStack(alignment: .leading, spacing: 10) {
                    ForEach([1, 2], id: \.self) { i in
                        PanelSection(title: "LFO \(i + 1)") {
                            VStack(alignment: .leading, spacing: 3) {
                                ForEach(["Waveform", "RateMode", "RateUnSynced", "RateSynced", "Fade", "PhaseDist", "PhaseInit", "LoopMode", "Retrig", "Unipolar", "Poly"], id: \.self) { p in
                                    ParamRow(cell: m["LFO\(i)_\(p)"], label: m["LFO\(i)_\(p)"].spec.name.replacingOccurrences(of: "LFO \(i + 1) ", with: ""), width: 250)
                                }
                            }
                        }
                    }
                    PanelSection(title: "Keyboard curves") {
                        VStack(alignment: .leading, spacing: 3) {
                            ForEach(1...4, id: \.self) { k in
                                HStack(spacing: 4) {
                                    Text(["Velocity", "Aftertouch", "Key track", "Mod wheel"][k - 1]).font(.system(size: 9)).foregroundStyle(Palette.text).frame(width: 60, alignment: .leading)
                                    ForEach(["Y0", "X1", "Y1", "Y2"], id: \.self) { p in
                                        CurveSlider(cell: m["Keyboard\(k)_\(p)"], label: p)
                                    }
                                }
                            }
                        }
                    }
                }
                PanelSection(title: "Modulation matrix") {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("Amounts apply to the destination's full range. Mod Mixer and Mod Seq slots are kept for Jup-8 V presets but not played.")
                            .font(.system(size: 9)).foregroundStyle(Palette.dim).frame(width: 520, alignment: .leading)
                        ForEach(Self.sources, id: \.0) { name, dests in
                            ForEach(Array(dests.enumerated()), id: \.offset) { k, d in
                                HStack(spacing: 6) {
                                    Text(k == 0 ? name : "").font(.system(size: 9, weight: .semibold)).foregroundStyle(Palette.orange).frame(width: 70, alignment: .leading)
                                    ParamRow(cell: m["ModMatrixDest\(d)"], label: "→", width: 220)
                                    ParamRow(cell: m["ModMatrixAmount_Src\(Self.srcIndex[name]!)_Dest\(d)"], label: "Amount", width: 220)
                                }
                            }
                        }
                    }
                }
            }
            .padding(10)
        }
    }
}

struct CurveSlider: View {
    @ObservedObject var cell: ParamCell
    let label: String
    var body: some View {
        HStack(spacing: 2) {
            Text(label).font(.system(size: 8)).foregroundStyle(Palette.dim)
            Slider(value: Binding(get: { cell.value }, set: { cell.set($0) }), in: 0...1).controlSize(.mini).frame(width: 60)
        }
    }
}

struct EffectsView: View {
    let m: ParamModel
    static let families = ["", "Dattorro_", "Delay_", "Chorus_", "Flanger_", "Phaser_", "TS808_", "Compressor_", "BitCrusher_", "MultiModeFilter_", "ParamEq_", "StereoPan_"]

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 8) {
                HStack(spacing: 12) {
                    Toggle2(cell: m["FX_Enable"], title: "EFFECTS ON", color: Palette.red)
                    Selector(cell: m["FX_Routing"], labels: ["SERIAL", "PARALLEL"], color: Palette.white, vertical: false)
                }
                HStack(alignment: .top, spacing: 12) {
                    ForEach(1...3, id: \.self) { slot in FXSlotView(m: m, slot: slot, type: m["FX\(slot)_ModuleType"]) }
                }
            }
            .padding(10)
        }
    }
}

struct FXSlotView: View {
    let m: ParamModel
    let slot: Int
    @ObservedObject var type: ParamCell

    var body: some View {
        PanelSection(title: "FX \(slot)") {
            VStack(alignment: .leading, spacing: 4) {
                HStack {
                    Toggle2(cell: m["FX\(slot)_Bypass"], title: "ON", color: Palette.yellow)
                    ParamRow(cell: type, label: "Type", width: 220)
                }
                ParamRow(cell: m["FX\(slot)_Dry_Wet"], label: "Dry/Wet", width: 300)
                let fam = EffectsView.families[type.index]
                if !fam.isEmpty {
                    ForEach(m.cells.filter { $0.spec.id.hasPrefix("FX\(slot)_\(fam)") }) { c in
                        ParamRow(cell: c, label: c.spec.name.replacingOccurrences(of: "FX\(slot) ", with: ""), width: 300)
                    }
                }
            }
        }
        .frame(width: 330, alignment: .topLeading)
    }
}
