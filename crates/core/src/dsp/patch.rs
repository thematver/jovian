//! Physical values of the patch, recomputed once per control block from the normalized values.

use super::modulation::rate_lists;
use super::voice_math as vm;
use crate::params::{addr as a, PARAMS};

#[derive(Debug, Clone, Copy)]
pub struct Patch {
    // LFO 1
    pub lfo_hz: f64,
    pub lfo_delay: f64,
    pub lfo_retrig: bool,
    pub lfo_wave: usize,
    // VCOs
    pub osc1_semis: f64,
    pub osc1_wave: usize,
    pub cross_mod: f64,
    pub osc2_semis: f64,
    pub osc2_fine: f64,
    pub osc2_wave: usize,
    pub osc2_low: bool,
    pub sync: usize,
    pub vco_lfo_st: f64,
    pub vco_env_st: f64,
    pub vco_dest: usize,
    pub pw_amount: f64,
    pub pw_mode: usize,
    pub mix: f64,
    // filters, VCA
    pub hpf_hz: f64,
    pub cutoff: f64,
    pub reso: f64,
    pub two_pole: bool,
    pub vcf_env: f64,
    pub vcf_env_sel: usize,
    pub vcf_lfo: f64,
    pub vcf_key: f64,
    pub vca_level: f64,
    pub vca_lfo: f64,
    // envelopes (ms / level)
    pub e1a: f64,
    pub e1d: f64,
    pub e1s: f64,
    pub e1r: f64,
    pub e1_kf: bool,
    pub e1_inv: bool,
    pub e2a: f64,
    pub e2d: f64,
    pub e2s: f64,
    pub e2r: f64,
    pub e2_kf: bool,
    // master & voice
    pub gain: f64,
    pub a4: f64,
    pub detune_st: f64,
    pub pan_spread: f64,
    pub porta_ms: f64,
    pub porta_on: bool,
    pub assign: usize,
    pub prio: usize,
    pub uni_voices: usize,
    pub polyphony: usize,
    pub disp_pitch: f64,
    pub disp_pw: f64,
    pub disp_env: f64,
    pub disp_cut: f64,
    pub disp_res: f64,
    pub disp_mod: f64,
    // arpeggio
    pub arp_on: bool,
    pub arp_mode: usize,
    pub arp_octaves: usize,
    pub arp_hz: f64,
    // bender & mod lever
    pub bend_v1: bool,
    pub bend_v2: bool,
    pub bend_st: f64,
    pub bend_f: bool,
    pub bend_f_amt: f64,
    pub mw_vco: bool,
    pub mw_vco_st: f64,
    pub mw_vcf: bool,
    pub mw_vcf_amt: f64,
}

impl Default for Patch {
    fn default() -> Self {
        Patch {
            lfo_hz: 1.0,
            lfo_delay: 0.0,
            lfo_retrig: false,
            lfo_wave: 0,
            osc1_semis: 0.0,
            osc1_wave: 1,
            cross_mod: 0.0,
            osc2_semis: 0.0,
            osc2_fine: 0.0,
            osc2_wave: 1,
            osc2_low: false,
            sync: 1,
            vco_lfo_st: 0.0,
            vco_env_st: 0.0,
            vco_dest: 1,
            pw_amount: 0.0,
            pw_mode: 1,
            mix: 0.5,
            hpf_hz: 5.0,
            cutoff: 1.0,
            reso: 0.0,
            two_pole: false,
            vcf_env: 0.0,
            vcf_env_sel: 0,
            vcf_lfo: 0.0,
            vcf_key: 0.0,
            vca_level: 1.0,
            vca_lfo: 0.0,
            e1a: 1.0,
            e1d: 1.0,
            e1s: 1.0,
            e1r: 1.0,
            e1_kf: false,
            e1_inv: false,
            e2a: 1.0,
            e2d: 1.0,
            e2s: 1.0,
            e2r: 1.0,
            e2_kf: false,
            gain: 1.0,
            a4: 440.0,
            detune_st: 0.0,
            pan_spread: 0.0,
            porta_ms: 0.0,
            porta_on: false,
            assign: 2,
            prio: 0,
            uni_voices: 0,
            polyphony: 8,
            disp_pitch: 0.0,
            disp_pw: 0.0,
            disp_env: 0.0,
            disp_cut: 0.0,
            disp_res: 0.0,
            disp_mod: 0.0,
            arp_on: false,
            arp_mode: 0,
            arp_octaves: 1,
            arp_hz: 4.0,
            bend_v1: true,
            bend_v2: true,
            bend_st: 2.0,
            bend_f: false,
            bend_f_amt: 0.0,
            mw_vco: false,
            mw_vco_st: 0.0,
            mw_vcf: false,
            mw_vcf_amt: 0.0,
        }
    }
}

impl Patch {
    pub fn update(&mut self, p: &[f32], tempo: f64) {
        let n = |i: usize| p[i] as f64;
        let phys = |i: usize| PARAMS[i].physical_f32(p[i]);
        let idx = |i: usize| PARAMS[i].index_f32(p[i]);
        let on = |i: usize| p[i] >= 0.5;

        self.lfo_hz = if on(a::LFO_MidiSync) {
            rate_lists::hz(rate_lists::LFO1_BARS[idx(a::LFO_RateSynced)], tempo)
        } else {
            phys(a::LFO_RateUnSynced)
        };
        self.lfo_delay = phys(a::LFO_FadeDelay);
        self.lfo_retrig = on(a::LFO_Retrig);
        self.lfo_wave = idx(a::LFO_Waveform);
        self.osc1_semis = (idx(a::Osc1Coarse) as f64 - 1.0) * 12.0;
        self.osc1_wave = idx(a::Osc1Wave);
        self.cross_mod = n(a::OscCrossMod);
        self.osc2_semis = idx(a::Osc2Coarse) as f64 - 12.0;
        self.osc2_fine = phys(a::Osc2Finetune);
        self.osc2_wave = idx(a::Osc2Wave);
        self.osc2_low = on(a::Osc2Mode);
        self.sync = idx(a::OscSyncSelect);
        self.vco_lfo_st = phys(a::OscLfoAmount);
        self.vco_env_st = phys(a::OscEnvAmount);
        self.vco_dest = idx(a::OscDestSelect);
        self.pw_amount = phys(a::OscPwmAmount) / 100.0;
        self.pw_mode = idx(a::OscPwmMode);
        self.mix = n(a::OscMix);
        self.hpf_hz = phys(a::HPF_Cutoff);
        self.cutoff = n(a::LpfCut);
        self.reso = n(a::LpfRes);
        self.two_pole = idx(a::LpfSlop) == 0;
        self.vcf_env = n(a::LpfEnvAmount);
        self.vcf_env_sel = idx(a::LpfEnvSource);
        self.vcf_lfo = n(a::LfoFiltAmount);
        self.vcf_key = n(a::LPF_KeyFollowAmount);
        self.vca_level = n(a::VCA_EnvAmount);
        self.vca_lfo = n(a::VCA_LfoAmount);
        self.e1a = phys(a::Env1_Attack);
        self.e1d = phys(a::Env1_Decay);
        self.e1s = phys(a::Env1_Sustain) / 100.0;
        self.e1r = phys(a::Env1_Release);
        self.e1_kf = on(a::Env1_KeyFollow);
        self.e1_inv = on(a::Env1_Polarity);
        self.e2a = phys(a::Env2_Attack);
        self.e2d = phys(a::Env2_Decay);
        self.e2s = phys(a::Env2_Sustain) / 100.0;
        self.e2r = phys(a::Env2Release);
        self.e2_kf = on(a::Env2_KeyFollow);
        self.gain = 10f64.powf(phys(a::Volume) / 20.0) * vm::OSC_LEVEL;
        self.a4 = phys(a::MasterTune);
        self.detune_st = phys(a::VoiceDetuneAmount) * vm::UNISON_DETUNE_SCALE;
        self.pan_spread = n(a::PanSpread);
        self.porta_ms = phys(a::Portamento_Time);
        self.porta_on = on(a::Portamento_On);
        self.assign = idx(a::AssignMode);
        self.prio = idx(a::PlPrio);
        self.uni_voices = idx(a::UniVPN);
        self.polyphony = idx(a::Polyphon) + 1;
        self.disp_pitch = phys(a::PitchDispersion);
        self.disp_pw = n(a::PWDispersion);
        self.disp_env = n(a::EnvDispersion);
        self.disp_cut = n(a::CutoffDispersion);
        self.disp_res = n(a::ResoDispersion);
        self.disp_mod = n(a::ModDispersion);
        self.arp_on = on(a::ArpSeq_ArpSeqMode);
        self.arp_mode = idx(a::ArpSeq_PatternMode);
        self.arp_octaves = idx(a::ArpSeq_OctaveRepeat) + 1;
        self.arp_hz = if on(a::ArpSeq_MidiSync) {
            rate_lists::hz(rate_lists::ARP_BARS[idx(a::ArpSeq_RateSynced)], tempo)
        } else {
            phys(a::ArpSeq_RateUnSynced)
        };
        self.bend_v1 = on(a::BendVCO1);
        self.bend_v2 = on(a::BendVCO2);
        self.bend_st = phys(a::BendVCOAmount);
        self.bend_f = on(a::BendVCF);
        self.bend_f_amt = n(a::BendVCFAmount);
        self.mw_vco = on(a::LfoModVCO);
        self.mw_vcf = on(a::LfoModVCF);
        self.mw_vcf_amt = n(a::LfoModVcfAmount);
        self.mw_vco_st = vm::MOD_WHEEL_PITCH_SEMITONES * n(a::LfoModVCOAmount).powf(vm::MOD_WHEEL_PITCH_EXPONENT);
    }
}
