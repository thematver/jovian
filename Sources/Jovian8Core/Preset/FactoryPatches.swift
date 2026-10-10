import Foundation

/// Built-in patches, written in physical units so they read like a Jupiter-8 patch sheet.
public enum FactoryPatches {
    public static let all: [PatchState] = [
        make("Init") { _ in },
        make("Poly Brass") { p in
            p.wave1("Saw"); p.wave2("Saw"); p.range2(semis: 0); p.fine2(0.08); p.set("OscMix", 0.5)
            p.cutoffHz(260); p.set("LpfRes", 0.18); p.set("LpfEnvAmount", 0.62); p.set("LPF_KeyFollowAmount", 0.5)
            p.env1(a: 70, d: 900, s: 0.45, r: 260); p.env2(a: 15, d: 1200, s: 0.85, r: 280)
            p.fx(1, "Chorus", mix: 0.35); p.fx(2, "Reverb", mix: 0.18)
        },
        make("String Machine") { p in
            p.wave1("Saw"); p.wave2("Saw"); p.range2(semis: 0); p.fine2(0.12); p.set("OscMix", 0.5)
            p.cutoffHz(2400); p.set("LpfEnvAmount", 0.1); p.item("LpfSlop", "12db")
            p.env2(a: 420, d: 1500, s: 0.9, r: 900); p.env1(a: 400, d: 2000, s: 0.6, r: 900)
            p.lfo(hz: 5.2, wave: "Sine", delay: 0.6); p.physical("OscLfoAmount", 0.12)
            p.fx(1, "Chorus", mix: 0.6); p.fx(2, "Reverb", mix: 0.3)
        },
        make("Solo Bass") { p in
            p.item("AssignMode", "Solo"); p.item("PlPrio", "Last")
            p.item("Osc1Coarse", "16'"); p.wave1("Square"); p.wave2("Saw"); p.range2(semis: -12); p.set("OscMix", 0.5)
            p.cutoffHz(120); p.set("LpfRes", 0.35); p.set("LpfEnvAmount", 0.55)
            p.env1(a: 1, d: 260, s: 0.1, r: 80); p.env2(a: 1, d: 600, s: 0.7, r: 60)
        },
        make("Sync Lead") { p in
            p.item("AssignMode", "Solo"); p.item("OscSyncSelect", "VCO 1"); p.item("OscDestSelect", "VCO 1")
            p.wave1("Saw"); p.wave2("Rectangle"); p.range2(semis: 0); p.set("OscMix", 0.25)
            p.physical("OscEnvAmount", 19); p.env1(a: 1, d: 700, s: 0.2, r: 200)
            p.cutoffHz(3000); p.set("LpfRes", 0.15); p.env2(a: 3, d: 800, s: 0.8, r: 250)
            p.item("Portamento_On", "On"); p.physical("Portamento_Time", 60)
            p.lfo(hz: 5.5, wave: "Sine", delay: 0.8); p.physical("OscLfoAmount", 0.15)
            p.fx(1, "Delay", mix: 0.22)
        },
        make("PWM Pad") { p in
            p.wave1("Rectangle"); p.wave2("Rectangle"); p.range2(semis: -12); p.fine2(0.06); p.set("OscMix", 0.45)
            p.item("OscPwmMode", "LFO Mode"); p.physical("OscPwmAmount", 70); p.lfo(hz: 0.6, wave: "Sine", delay: 0)
            p.cutoffHz(1500); p.set("LpfRes", 0.1); p.env2(a: 700, d: 2000, s: 0.85, r: 1600)
            p.fx(1, "Chorus", mix: 0.4); p.fx(2, "Reverb", mix: 0.35)
        },
        make("Arp Pluck") { p in
            p.item("ArpSeq_ArpSeqMode", "On"); p.item("ArpSeq_PatternMode", "Up"); p.item("ArpSeq_OctaveRepeat", "2")
            p.item("ArpSeq_MidiSync", "Sync"); p.item("ArpSeq_RateSynced", "1/16 Bar")
            p.wave1("Saw"); p.wave2("Rectangle"); p.set("OscMix", 0.4)
            p.cutoffHz(400); p.set("LpfRes", 0.3); p.set("LpfEnvAmount", 0.5); p.env1(a: 1, d: 180, s: 0, r: 150)
            p.env2(a: 1, d: 400, s: 0.3, r: 200); p.fx(1, "Delay", mix: 0.3)
        },
        make("Unison Lead") { p in
            p.item("AssignMode", "Unison"); p.physical("VoiceDetuneAmount", 0.18); p.set("PanSpread", 0.6)
            p.item("Polyphon", "4"); p.wave1("Saw"); p.wave2("Saw"); p.range2(semis: -12); p.set("OscMix", 0.5)
            p.cutoffHz(1800); p.set("LpfRes", 0.2); p.set("LpfEnvAmount", 0.3); p.env1(a: 5, d: 500, s: 0.4, r: 300)
            p.env2(a: 5, d: 600, s: 0.9, r: 300); p.physical("Volume", -14)
        },
        make("Cross Mod Bell") { p in
            p.wave1("Triangle"); p.wave2("Sine"); p.range2(semis: 19); p.set("OscCrossMod", 0.35); p.set("OscMix", 0.15)
            p.cutoffHz(5000); p.env2(a: 1, d: 2500, s: 0, r: 2000); p.env1(a: 1, d: 1500, s: 0, r: 1500)
            p.item("Env2_KeyFollow", "On"); p.fx(1, "Reverb", mix: 0.35)
        },
        make("Filter Sweep") { p in
            p.wave1("Saw"); p.wave2("Saw"); p.range2(semis: 7); p.set("OscMix", 0.4)
            p.cutoffHz(300); p.set("LpfRes", 0.75); p.set("LfoFiltAmount", 0.55); p.lfo(hz: 0.25, wave: "Sine", delay: 0)
            p.env2(a: 200, d: 2000, s: 0.9, r: 1500); p.fx(1, "Phaser", mix: 0.3); p.fx(2, "Delay", mix: 0.25)
        },
    ]

    static func make(_ name: String, _ build: (inout Builder) -> Void) -> PatchState {
        var b = Builder(state: PatchState(name: name, pack: "Jovian-8", author: "Jovian-8"))
        b.item("FX_Enable", "On")
        for slot in 1...3 { b.item("FX\(slot)_ModuleType", "None") }
        build(&b)
        return b.state
    }

    struct Builder {
        var state: PatchState

        mutating func set(_ id: String, _ normalized: Double) { state.values[id] = normalized }
        mutating func physical(_ id: String, _ v: Double) { state.values[id] = ParamTable.spec(id).normalized(fromPhysical: v) }
        mutating func item(_ id: String, _ label: String) {
            let s = ParamTable.spec(id)
            guard let i = s.items.firstIndex(of: label) else { preconditionFailure("\(id) has no item \(label)") }
            state.values[id] = ParamSpec.normalized(index: i, count: s.itemCount)
        }
        mutating func wave1(_ w: String) { item("Osc1Wave", w) }
        mutating func wave2(_ w: String) { item("Osc2Wave", w) }
        mutating func range2(semis: Int) { item("Osc2Coarse", String(semis)) }
        mutating func fine2(_ st: Double) { physical("Osc2Finetune", st) }
        mutating func cutoffHz(_ hz: Double) {
            set("LpfCut", log2(max(hz - VoiceMath.cutoffOffsetHz, 1) / VoiceMath.cutoffBaseHz) / VoiceMath.cutoffOctaves)
        }
        mutating func env1(a: Double, d: Double, s: Double, r: Double) {
            physical("Env1_Attack", max(a, 0.8)); physical("Env1_Decay", max(d, 1.5)); physical("Env1_Sustain", s * 100); physical("Env1_Release", max(r, 1.5))
        }
        mutating func env2(a: Double, d: Double, s: Double, r: Double) {
            physical("Env2_Attack", max(a, 0.8)); physical("Env2_Decay", max(d, 1.5)); physical("Env2_Sustain", s * 100); physical("Env2Release", max(r, 1.5))
        }
        mutating func lfo(hz: Double, wave: String, delay: Double) {
            physical("LFO_RateUnSynced", hz); item("LFO_Waveform", wave); physical("LFO_FadeDelay", delay)
        }
        mutating func fx(_ slot: Int, _ type: String, mix: Double) {
            item("FX\(slot)_ModuleType", type)
            item("FX\(slot)_Bypass", "On")
            set("FX\(slot)_Dry_Wet", mix)
        }
    }
}
