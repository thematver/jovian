import Foundation

/// Parameter addresses used by the engine, resolved once.
enum A {
    static let lfoRate = ParamTable.address("LFO_RateUnSynced"), lfoRateSync = ParamTable.address("LFO_RateSynced")
    static let lfoSync = ParamTable.address("LFO_MidiSync"), lfoDelay = ParamTable.address("LFO_FadeDelay")
    static let lfoRetrig = ParamTable.address("LFO_Retrig"), lfoWave = ParamTable.address("LFO_Waveform")
    static let osc1Range = ParamTable.address("Osc1Coarse"), osc1Wave = ParamTable.address("Osc1Wave")
    static let crossMod = ParamTable.address("OscCrossMod"), osc2Coarse = ParamTable.address("Osc2Coarse")
    static let osc2Fine = ParamTable.address("Osc2Finetune"), osc2Wave = ParamTable.address("Osc2Wave")
    static let osc2Low = ParamTable.address("Osc2Mode"), sync = ParamTable.address("OscSyncSelect")
    static let vcoLfo = ParamTable.address("OscLfoAmount"), vcoEnv = ParamTable.address("OscEnvAmount")
    static let vcoDest = ParamTable.address("OscDestSelect"), pwAmount = ParamTable.address("OscPwmAmount")
    static let pwMode = ParamTable.address("OscPwmMode"), mix = ParamTable.address("OscMix")
    static let hpf = ParamTable.address("HPF_Cutoff"), cutoff = ParamTable.address("LpfCut")
    static let reso = ParamTable.address("LpfRes"), slope = ParamTable.address("LpfSlop")
    static let vcfEnv = ParamTable.address("LpfEnvAmount"), vcfEnvSel = ParamTable.address("LpfEnvSource")
    static let vcfLfo = ParamTable.address("LfoFiltAmount"), vcfKey = ParamTable.address("LPF_KeyFollowAmount")
    static let vcaLevel = ParamTable.address("VCA_EnvAmount"), vcaLfo = ParamTable.address("VCA_LfoAmount")
    static let e1A = ParamTable.address("Env1_Attack"), e1D = ParamTable.address("Env1_Decay")
    static let e1S = ParamTable.address("Env1_Sustain"), e1R = ParamTable.address("Env1_Release")
    static let e1KF = ParamTable.address("Env1_KeyFollow"), e1Inv = ParamTable.address("Env1_Polarity")
    static let e2A = ParamTable.address("Env2_Attack"), e2D = ParamTable.address("Env2_Decay")
    static let e2S = ParamTable.address("Env2_Sustain"), e2R = ParamTable.address("Env2Release")
    static let e2KF = ParamTable.address("Env2_KeyFollow")
    static let volume = ParamTable.address("Volume"), tune = ParamTable.address("MasterTune")
    static let detune = ParamTable.address("VoiceDetuneAmount"), pan = ParamTable.address("PanSpread")
    static let portaTime = ParamTable.address("Portamento_Time"), portaOn = ParamTable.address("Portamento_On")
    static let assign = ParamTable.address("AssignMode"), prio = ParamTable.address("PlPrio")
    static let uniVoices = ParamTable.address("UniVPN"), poly = ParamTable.address("Polyphon")
    static let dispPitch = ParamTable.address("PitchDispersion"), dispPW = ParamTable.address("PWDispersion")
    static let dispEnv = ParamTable.address("EnvDispersion"), dispCut = ParamTable.address("CutoffDispersion")
    static let dispRes = ParamTable.address("ResoDispersion"), dispMod = ParamTable.address("ModDispersion")
    static let arpOn = ParamTable.address("ArpSeq_ArpSeqMode"), arpMode = ParamTable.address("ArpSeq_PatternMode")
    static let arpOct = ParamTable.address("ArpSeq_OctaveRepeat"), arpRate = ParamTable.address("ArpSeq_RateUnSynced")
    static let arpRateSync = ParamTable.address("ArpSeq_RateSynced"), arpSync = ParamTable.address("ArpSeq_MidiSync")
    static let bendV1 = ParamTable.address("BendVCO1"), bendV2 = ParamTable.address("BendVCO2")
    static let bendAmt = ParamTable.address("BendVCOAmount"), bendF = ParamTable.address("BendVCF")
    static let bendFAmt = ParamTable.address("BendVCFAmount")
    static let mwVCO = ParamTable.address("LfoModVCO"), mwVCOAmt = ParamTable.address("LfoModVCOAmount")
    static let mwVCF = ParamTable.address("LfoModVCF"), mwVCFAmt = ParamTable.address("LfoModVcfAmount")
    static let fxMain = [ParamTable.address("FX1_Dry_Wet"), ParamTable.address("FX2_Dry_Wet"), ParamTable.address("FX3_Dry_Wet")]
}

/// Index lists for the bar-based rate lists of Jup-8 V (bars per cycle / per step).
enum RateLists {
    static let lfo1Bars: [Double] = [8, 6, 4, 3, 2, 1.5, 1, 0.75, 0.5, 0.375, 1.0 / 3, 5.0 / 16, 0.25, 3.0 / 16,
                                     1.0 / 6, 0.125, 1.0 / 12, 1.0 / 16, 1.0 / 24, 1.0 / 32]
    static let hybridBars: [Double] = [8, 4, 2, 1, 0.5, 0.25, 0.125, 1.0 / 16, 1.0 / 32]
    static let arpBars: [Double] = [1, 0.75, 0.5, 0.375, 1.0 / 3, 5.0 / 16, 0.25, 3.0 / 16, 1.0 / 6, 0.125,
                                    1.0 / 12, 1.0 / 16, 1.0 / 24, 1.0 / 32, 1.0 / 48, 1.0 / 64]

    @inline(__always)
    static func hz(bars: Double, tempo: Double) -> Double { tempo / 240 / max(bars, 1e-6) }
}

/// Physical values of the patch, recomputed once per control block from the normalized store.
struct Patch {
    // LFO 1
    var lfoHz = 1.0, lfoDelay = 0.0, lfoRetrig = false, lfoWave = 0
    // VCOs
    var osc1Semis = 0.0, osc1Wave = 1, crossMod = 0.0
    var osc2Semis = 0.0, osc2Fine = 0.0, osc2Wave = 1, osc2Low = false, sync = 1
    var vcoLfoSt = 0.0, vcoEnvSt = 0.0, vcoDest = 1, pwAmount = 0.0, pwMode = 1, mix = 0.5
    // filters, VCA
    var hpfHz = 5.0, cutoff = 1.0, reso = 0.0, twoPole = false, vcfEnv = 0.0, vcfEnvSel = 0, vcfLfo = 0.0, vcfKey = 0.0
    var vcaLevel = 1.0, vcaLfo = 0.0
    // envelopes (ms / level)
    var e1A = 1.0, e1D = 1.0, e1S = 1.0, e1R = 1.0, e1KF = false, e1Inv = false
    var e2A = 1.0, e2D = 1.0, e2S = 1.0, e2R = 1.0, e2KF = false
    // master & voice
    var gain = 1.0, a4 = 440.0, detuneSt = 0.0, panSpread = 0.0, portaMs = 0.0, portaOn = false
    var assign = 2, prio = 0, uniVoices = 0, polyphony = 8
    var dispPitch = 0.0, dispPW = 0.0, dispEnv = 0.0, dispCut = 0.0, dispRes = 0.0, dispMod = 0.0
    // arpeggio
    var arpOn = false, arpMode = 0, arpOctaves = 1, arpHz = 4.0
    // bender & mod lever
    var bendV1 = true, bendV2 = true, bendSt = 2.0, bendF = false, bendFAmt = 0.0
    var mwVCO = false, mwVCOSt = 0.0, mwVCF = false, mwVCFAmt = 0.0

    mutating func update(_ p: UnsafePointer<Float>, tempo: Double) {
        @inline(__always) func n(_ a: Int) -> Double { Double(p[a]) }
        @inline(__always) func phys(_ a: Int) -> Double { ParamTable.fast[a].physical(p[a]) }
        @inline(__always) func idx(_ a: Int) -> Int { ParamTable.fast[a].index(p[a]) }
        @inline(__always) func on(_ a: Int) -> Bool { p[a] >= 0.5 }

        lfoHz = on(A.lfoSync) ? RateLists.hz(bars: RateLists.lfo1Bars[idx(A.lfoRateSync)], tempo: tempo) : phys(A.lfoRate)
        lfoDelay = phys(A.lfoDelay); lfoRetrig = on(A.lfoRetrig); lfoWave = idx(A.lfoWave)
        osc1Semis = Double(idx(A.osc1Range) - 1) * 12
        osc1Wave = idx(A.osc1Wave); crossMod = n(A.crossMod)
        osc2Semis = Double(idx(A.osc2Coarse) - 12); osc2Fine = phys(A.osc2Fine)
        osc2Wave = idx(A.osc2Wave); osc2Low = on(A.osc2Low); sync = idx(A.sync)
        vcoLfoSt = phys(A.vcoLfo); vcoEnvSt = phys(A.vcoEnv); vcoDest = idx(A.vcoDest)
        pwAmount = phys(A.pwAmount) / 100; pwMode = idx(A.pwMode); mix = n(A.mix)
        hpfHz = phys(A.hpf); cutoff = n(A.cutoff); reso = n(A.reso); twoPole = idx(A.slope) == 0
        vcfEnv = n(A.vcfEnv); vcfEnvSel = idx(A.vcfEnvSel); vcfLfo = n(A.vcfLfo); vcfKey = n(A.vcfKey)
        vcaLevel = n(A.vcaLevel); vcaLfo = n(A.vcaLfo)
        e1A = phys(A.e1A); e1D = phys(A.e1D); e1S = phys(A.e1S) / 100; e1R = phys(A.e1R)
        e1KF = on(A.e1KF); e1Inv = on(A.e1Inv)
        e2A = phys(A.e2A); e2D = phys(A.e2D); e2S = phys(A.e2S) / 100; e2R = phys(A.e2R); e2KF = on(A.e2KF)
        gain = pow(10, phys(A.volume) / 20) * VoiceMath.oscLevel
        a4 = phys(A.tune); detuneSt = phys(A.detune) * VoiceMath.unisonDetuneScale; panSpread = n(A.pan)
        portaMs = phys(A.portaTime); portaOn = on(A.portaOn)
        assign = idx(A.assign); prio = idx(A.prio); uniVoices = idx(A.uniVoices); polyphony = idx(A.poly) + 1
        dispPitch = phys(A.dispPitch); dispPW = n(A.dispPW); dispEnv = n(A.dispEnv)
        dispCut = n(A.dispCut); dispRes = n(A.dispRes); dispMod = n(A.dispMod)
        arpOn = on(A.arpOn); arpMode = idx(A.arpMode); arpOctaves = idx(A.arpOct) + 1
        arpHz = on(A.arpSync) ? RateLists.hz(bars: RateLists.arpBars[idx(A.arpRateSync)], tempo: tempo) : phys(A.arpRate)
        bendV1 = on(A.bendV1); bendV2 = on(A.bendV2); bendSt = phys(A.bendAmt); bendF = on(A.bendF); bendFAmt = n(A.bendFAmt)
        mwVCO = on(A.mwVCO); mwVCF = on(A.mwVCF); mwVCFAmt = n(A.mwVCFAmt)
        mwVCOSt = VoiceMath.modWheelPitchSemitones * pow(n(A.mwVCOAmt), VoiceMath.modWheelPitchExponent)
    }
}

/// Extra VoiceMath used by Patch.
extension VoiceMath {
    public static let unisonDetuneScale = 1.0
}
