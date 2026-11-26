/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Procedural onboarding audio synthesizer backed by `cpal`.
//!
//! Maps each of the six Voronoi mode cells to a C-major pentatonic pitch with soft sine/triangle oscillators, subtle
//! jelly pitch glides, and a 3.4 kHz low-pass filter. Opens the host audio device lazily on first unmuted cue and
//! degrades silently to a no-op if no audio output device is available (such as under headless Xvfb).

use crate::ui::onboarding::theme::{cos_f32, pow_pos_f32, sin_f32};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::f32::consts::TAU;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use zendia_browser_core::model::InputMode;

/// Master output level matching the fork's `sound.js` gain staging.
const MASTER_GAIN: f32 = 0.24;
/// Low-pass cutoff frequency in Hz.
const LOWPASS_HZ: f32 = 3400.0;
/// Low-pass filter Q factor.
const LOWPASS_Q: f32 = 0.7;
/// Maximum simultaneously active voices to keep the callback lock-free and bounded.
const MAX_VOICES: usize = 32;

/// Ascending C-major pentatonic ladder for pinning Essential app tiles.
const PIN_SCALE_HZ: [f32; 9] = [523.25, 587.33, 659.25, 783.99, 880.0, 1046.5, 1174.66, 1318.51, 1567.98];

/// Fundamental pitch associated with each Voronoi mascot mode cell.
pub fn mode_frequency_hz(mode: InputMode) -> f32 {
    match mode {
        InputMode::Normal | InputMode::Window => 523.25, // C5
        InputMode::Insert => 587.33,                     // D5
        InputMode::Visual => 659.25,                     // E5
        InputMode::Explorer => 783.99,                   // G5
        InputMode::Toolbar => 880.0,                     // A5
        InputMode::Library => 1046.5,                    // C6
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Waveform {
    Sine,
    Triangle,
}

#[derive(Clone, Copy, Debug)]
struct Voice {
    waveform: Waveform,
    start_freq: f32,
    mid_freq: f32,
    end_freq: f32,
    delay_secs: f32,
    attack_secs: f32,
    duration_secs: f32,
    gain: f32,
    harmonic: f32,
    elapsed_secs: f32,
    phase: f32,
    over_phase: f32,
    is_tick: bool,
}

impl Voice {
    fn tone(freq: f32, start_freq: f32, end_freq: f32, delay: f32, duration: f32, gain: f32, harmonic: f32) -> Self {
        Self {
            waveform: Waveform::Sine,
            start_freq,
            mid_freq: freq,
            end_freq,
            delay_secs: delay.max(0.0),
            attack_secs: 0.008,
            duration_secs: duration.max(0.01),
            gain,
            harmonic,
            elapsed_secs: 0.0,
            phase: 0.0,
            over_phase: 0.0,
            is_tick: false,
        }
    }

    fn tick(freq: f32, delay: f32, duration: f32, gain: f32) -> Self {
        Self {
            waveform: Waveform::Triangle,
            start_freq: freq,
            mid_freq: freq,
            end_freq: freq * 0.42,
            delay_secs: delay.max(0.0),
            attack_secs: 0.003,
            duration_secs: duration.max(0.008),
            gain,
            harmonic: 0.0,
            elapsed_secs: 0.0,
            phase: 0.0,
            over_phase: 0.0,
            is_tick: true,
        }
    }

    fn current_freq(&self, local_t: f32) -> f32 {
        if self.is_tick {
            let p = (local_t / self.duration_secs).clamp(0.0, 1.0);
            return self.start_freq * pow_pos_f32(self.end_freq / self.start_freq, p);
        }
        if (self.start_freq - self.mid_freq).abs() > 0.01 {
            let t1 = (0.055_f32).min(self.duration_secs * 0.35);
            let t2 = (0.12_f32).min(self.duration_secs * 0.65);
            let overshoot = self.mid_freq * 1.025;
            if local_t <= t1 && t1 > 1e-5 {
                let p = (local_t / t1).clamp(0.0, 1.0);
                self.start_freq * pow_pos_f32(overshoot / self.start_freq, p)
            } else if local_t <= t2 && (t2 - t1) > 1e-5 {
                let p = ((local_t - t1) / (t2 - t1)).clamp(0.0, 1.0);
                overshoot * pow_pos_f32(self.end_freq / overshoot, p)
            } else {
                self.end_freq
            }
        } else if (self.end_freq - self.mid_freq).abs() > 0.01 {
            let p = (local_t / self.duration_secs).clamp(0.0, 1.0);
            self.mid_freq * pow_pos_f32(self.end_freq / self.mid_freq, p)
        } else {
            self.mid_freq
        }
    }

    fn sample(&mut self, dt: f32) -> Option<f32> {
        let total = self.delay_secs + self.duration_secs + 0.015;
        if self.elapsed_secs >= total {
            return None;
        }
        let now = self.elapsed_secs;
        self.elapsed_secs += dt;
        if now < self.delay_secs {
            return Some(0.0);
        }
        let local_t = now - self.delay_secs;
        if local_t >= self.duration_secs {
            return Some(0.0);
        }

        let env = if local_t < self.attack_secs {
            (local_t / self.attack_secs).clamp(0.0, 1.0)
        } else {
            let decay_t =
                ((local_t - self.attack_secs) / (self.duration_secs - self.attack_secs).max(0.001)).clamp(0.0, 1.0);
            // Smooth exponential decay down toward -80 dB.
            pow_pos_f32(0.0001_f32, decay_t)
        };

        let freq = self.current_freq(local_t);
        self.phase = (self.phase + freq * dt) % 1.0;
        let base = match self.waveform {
            Waveform::Sine => sin_f32(self.phase * TAU),
            Waveform::Triangle => triangle_wave(self.phase),
        };
        let mut out = base * env * self.gain;

        if self.harmonic > 0.0 && local_t < self.duration_secs * 0.72 {
            let over_dur = (self.duration_secs * 0.72).max(0.005);
            let over_env = if local_t < self.attack_secs {
                (local_t / self.attack_secs).clamp(0.0, 1.0)
            } else {
                let p = ((local_t - self.attack_secs) / (over_dur - self.attack_secs).max(0.001)).clamp(0.0, 1.0);
                pow_pos_f32(0.0001_f32, p)
            };
            self.over_phase = (self.over_phase + (self.mid_freq * 2.0) * dt) % 1.0;
            out += triangle_wave(self.over_phase) * over_env * self.gain * self.harmonic;
        }
        Some(out)
    }
}

fn triangle_wave(phase: f32) -> f32 {
    let p = phase.rem_euclid(1.0);
    if p < 0.25 {
        4.0 * p
    } else if p < 0.75 {
        2.0 - 4.0 * p
    } else {
        4.0 * p - 4.0
    }
}

/// Second-order biquad low-pass filter matching Web Audio's `BiquadFilterNode` (`lowpass`, 3.4 kHz, Q=0.7).
#[derive(Clone, Copy, Debug)]
struct BiquadLowpass {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl BiquadLowpass {
    fn new(sample_rate: f32, cutoff_hz: f32, q: f32) -> Self {
        let clamped_cutoff = cutoff_hz.clamp(80.0, (sample_rate * 0.45).max(120.0));
        let w0 = TAU * clamped_cutoff / sample_rate.max(8000.0);
        let cos_w0 = cos_f32(w0);
        let alpha = sin_f32(w0) / (2.0 * q.max(0.1));
        let a0 = 1.0 + alpha;
        Self {
            b0: ((1.0 - cos_w0) * 0.5) / a0,
            b1: (1.0 - cos_w0) / a0,
            b2: ((1.0 - cos_w0) * 0.5) / a0,
            a1: (-2.0 * cos_w0) / a0,
            a2: (1.0 - alpha) / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

/// Pure-Rust polyphonic synthesizer state shared with the `cpal` audio callback.
pub struct SynthState {
    voices: Vec<Voice>,
    filter: BiquadLowpass,
    sample_rate: f32,
    muted: bool,
    master_gain_current: f32,
    master_gain_target: f32,
}

impl SynthState {
    pub fn new(sample_rate: f32, muted: bool) -> Self {
        let initial_gain = if muted { 0.0 } else { 1.0 };
        Self {
            voices: Vec::with_capacity(MAX_VOICES),
            filter: BiquadLowpass::new(sample_rate, LOWPASS_HZ, LOWPASS_Q),
            sample_rate: sample_rate.max(8000.0),
            muted,
            master_gain_current: initial_gain,
            master_gain_target: initial_gain,
        }
    }

    fn push_voice(&mut self, voice: Voice) {
        if self.voices.len() >= MAX_VOICES {
            self.voices.remove(0);
        }
        self.voices.push(voice);
    }

    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
        self.master_gain_target = if muted { 0.0 } else { 1.0 };
    }

    pub fn enqueue_mode(&mut self, mode: InputMode) {
        let f = mode_frequency_hz(mode);
        self.push_voice(Voice::tone(f, f * 0.84, f, 0.0, 0.24, 0.34, 0.16));
    }

    pub fn enqueue_start(&mut self) {
        let chord = [523.25, 659.25, 783.99, 1046.5];
        for (i, f) in chord.into_iter().enumerate() {
            self.push_voice(Voice::tone(
                f,
                f * 0.96,
                f,
                i as f32 * 0.055,
                0.48,
                0.24 - i as f32 * 0.02,
                0.18,
            ));
        }
    }

    pub fn enqueue_step(&mut self, mode: InputMode, direction: i8) {
        let target = mode_frequency_hz(mode);
        if direction >= 0 {
            self.push_voice(Voice::tone(
                target * 0.75,
                target * 0.75,
                target * 0.75,
                0.0,
                0.14,
                0.18,
                0.08,
            ));
            self.push_voice(Voice::tone(target, target * 0.9, target, 0.055, 0.26, 0.30, 0.16));
        } else {
            self.push_voice(Voice::tone(
                target * 1.12,
                target * 1.12,
                target * 1.12,
                0.0,
                0.13,
                0.18,
                0.06,
            ));
            self.push_voice(Voice::tone(target, target * 1.05, target, 0.05, 0.22, 0.24, 0.10));
        }
    }

    pub fn enqueue_select(&mut self, positive: bool) {
        self.push_voice(Voice::tick(if positive { 1600.0 } else { 1050.0 }, 0.0, 0.028, 0.09));
        let f = if positive { 659.25 } else { 440.0 };
        let start = if positive { f * 0.88 } else { f * 1.08 };
        self.push_voice(Voice::tone(f, start, f, 0.0, 0.16, 0.24, 0.12));
    }

    pub fn enqueue_app_pin(&mut self, pinned: bool, count: usize) {
        let idx = count.saturating_sub(1).min(PIN_SCALE_HZ.len() - 1);
        let f = PIN_SCALE_HZ[idx];
        if pinned {
            self.push_voice(Voice::tick(1700.0, 0.0, 0.028, 0.08));
            self.push_voice(Voice::tone(f, f * 0.86, f, 0.0, 0.22, 0.28, 0.20));
        } else {
            self.push_voice(Voice::tone(f * 0.75, f * 0.85, f * 0.70, 0.0, 0.15, 0.18, 0.06));
        }
    }

    pub fn enqueue_preview_key(&mut self, mode: InputMode) {
        let f = mode_frequency_hz(mode) * 1.5;
        self.push_voice(Voice::tick(f, 0.0, 0.022, 0.045));
    }

    pub fn enqueue_deflect(&mut self) {
        self.push_voice(Voice::tone(1174.66, 987.77, 1174.66, 0.0, 0.11, 0.075, 0.10));
    }

    pub fn enqueue_finish(&mut self) {
        let chord = [523.25, 659.25, 783.99, 987.77, 1174.66];
        for (i, f) in chord.into_iter().enumerate() {
            self.push_voice(Voice::tone(
                f,
                f * 0.98,
                f,
                i as f32 * 0.048,
                0.68,
                0.22 - i as f32 * 0.018,
                0.22,
            ));
        }
    }

    /// Renders `out.len()` mono samples into `out`.
    pub fn render_mono(&mut self, out: &mut [f32]) {
        if (self.muted && self.master_gain_current <= 1e-4) || self.voices.is_empty() {
            if self.muted && self.master_gain_current <= 1e-4 {
                self.master_gain_current = 0.0;
                self.voices.clear();
            }
            out.fill(0.0);
            return;
        }
        let dt = 1.0 / self.sample_rate;
        // 20 ms linear master gain ramp matching sound.js mute/unmute smoothing.
        let ramp_step = 1.0 / (0.020 * self.sample_rate).max(1.0);
        for sample in out.iter_mut() {
            if self.master_gain_current < self.master_gain_target {
                self.master_gain_current = (self.master_gain_current + ramp_step).min(self.master_gain_target);
            } else if self.master_gain_current > self.master_gain_target {
                self.master_gain_current = (self.master_gain_current - ramp_step).max(self.master_gain_target);
            }
            let mut sum = 0.0_f32;
            let mut i = 0;
            while i < self.voices.len() {
                if let Some(v) = self.voices[i].sample(dt) {
                    sum += v;
                    i += 1;
                } else {
                    self.voices.swap_remove(i);
                }
            }
            let filtered = self.filter.process(sum) * MASTER_GAIN * self.master_gain_current;
            *sample = filtered.clamp(-0.95, 0.95);
        }
        if self.muted && self.master_gain_current <= 1e-4 {
            self.master_gain_current = 0.0;
            self.voices.clear();
        }
    }
}

/// Process-wide lazy `cpal` audio backend that keeps the stream alive without ever crashing when audio hardware is
/// absent.
struct AudioHandle {
    synth: Arc<Mutex<SynthState>>,
    muted: AtomicBool,
    init_attempted: AtomicBool,
    available: Arc<AtomicBool>,
}

impl AudioHandle {
    fn new() -> Self {
        let headless = std::env::var_os("ZENDIA_NO_AUDIO").is_some();
        Self {
            synth: Arc::new(Mutex::new(SynthState::new(48_000.0, headless))),
            muted: AtomicBool::new(headless),
            init_attempted: AtomicBool::new(headless),
            available: Arc::new(AtomicBool::new(false)),
        }
    }

    fn ensure_stream_started(&self) {
        if self.muted.load(Ordering::Relaxed) {
            return;
        }
        if self
            .init_attempted
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        let synth = Arc::clone(&self.synth);
        let available = Arc::clone(&self.available);
        // Open and hold the `!Send` cpal::Stream directly on a dedicated audio thread so opening never blocks the UI
        // and no platform-specific Send requirements are violated.
        let _ = std::thread::Builder::new()
            .name("zendia-onboarding-audio".to_string())
            .spawn(move || {
                let opened = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
                    let host = cpal::default_host();
                    let device = host.default_output_device()?;
                    let config = device.default_output_config().ok()?;
                    let sample_rate = config.sample_rate().0 as f32;
                    let channels = config.channels() as usize;
                    if let Ok(mut s) = synth.lock() {
                        s.sample_rate = sample_rate.max(8000.0);
                        s.filter = BiquadLowpass::new(s.sample_rate, LOWPASS_HZ, LOWPASS_Q);
                    }
                    let synth_cb = Arc::clone(&synth);
                    let err_fn = |_err| {};
                    let stream = match config.sample_format() {
                        cpal::SampleFormat::F32 => device.build_output_stream(
                            &config.into(),
                            move |data: &mut [f32], _| fill_interleaved_f32(&synth_cb, data, channels),
                            err_fn,
                            None,
                        ),
                        cpal::SampleFormat::I16 => device.build_output_stream(
                            &config.into(),
                            move |data: &mut [i16], _| fill_interleaved_i16(&synth_cb, data, channels),
                            err_fn,
                            None,
                        ),
                        cpal::SampleFormat::U16 => device.build_output_stream(
                            &config.into(),
                            move |data: &mut [u16], _| fill_interleaved_u16(&synth_cb, data, channels),
                            err_fn,
                            None,
                        ),
                        _ => return None,
                    }
                    .ok()?;
                    stream.play().ok()?;
                    Some(stream)
                }));
                if let Ok(Some(stream)) = opened {
                    available.store(true, Ordering::Relaxed);
                    let _keep_alive = stream;
                    loop {
                        std::thread::park();
                    }
                }
            });
    }

    fn with_synth(&self, f: impl FnOnce(&mut SynthState)) {
        if self.muted.load(Ordering::Relaxed) {
            return;
        }
        self.ensure_stream_started();
        // Enqueue immediately even while the audio thread is opening the cpal stream so the very first cue plays
        // without being dropped. If audio initialization ultimately fails (headless), voices remain bounded by MAX_VOICES.
        if let Ok(mut synth) = self.synth.lock() {
            f(&mut synth);
        }
    }
}

fn fill_interleaved_f32(synth: &Arc<Mutex<SynthState>>, data: &mut [f32], channels: usize) {
    let ch = channels.max(1);
    let frames = data.len() / ch;
    let mut mono = vec![0.0_f32; frames];
    if let Ok(mut state) = synth.try_lock() {
        state.render_mono(&mut mono);
    }
    for (frame_idx, sample) in mono.into_iter().enumerate() {
        let base = frame_idx * ch;
        for c in 0..ch {
            data[base + c] = sample;
        }
    }
}

fn fill_interleaved_i16(synth: &Arc<Mutex<SynthState>>, data: &mut [i16], channels: usize) {
    let ch = channels.max(1);
    let frames = data.len() / ch;
    let mut mono = vec![0.0_f32; frames];
    if let Ok(mut state) = synth.try_lock() {
        state.render_mono(&mut mono);
    }
    for (frame_idx, sample) in mono.into_iter().enumerate() {
        let s = (sample * i16::MAX as f32).clamp(i16::MIN as f32, i16::MAX as f32) as i16;
        let base = frame_idx * ch;
        for c in 0..ch {
            data[base + c] = s;
        }
    }
}

fn fill_interleaved_u16(synth: &Arc<Mutex<SynthState>>, data: &mut [u16], channels: usize) {
    let ch = channels.max(1);
    let frames = data.len() / ch;
    let mut mono = vec![0.0_f32; frames];
    if let Ok(mut state) = synth.try_lock() {
        state.render_mono(&mut mono);
    }
    for (frame_idx, sample) in mono.into_iter().enumerate() {
        let s = ((sample * 0.5 + 0.5) * u16::MAX as f32).clamp(0.0, u16::MAX as f32) as u16;
        let base = frame_idx * ch;
        for c in 0..ch {
            data[base + c] = s;
        }
    }
}

static AUDIO: OnceLock<AudioHandle> = OnceLock::new();

fn handle() -> &'static AudioHandle {
    AUDIO.get_or_init(AudioHandle::new)
}

/// Pre-warms the background audio output stream when onboarding opens unmuted so cues play without startup latency.
pub fn warm_up() {
    handle().ensure_stream_started();
}

/// Updates the global onboarding mute state.
pub fn set_muted(muted: bool) {
    let h = handle();
    h.muted.store(muted, Ordering::Relaxed);
    if let Ok(mut synth) = h.synth.lock() {
        synth.set_muted(muted);
    }
    if !muted {
        h.ensure_stream_started();
    }
}

/// Returns whether onboarding audio is currently muted.
pub fn is_muted() -> bool {
    handle().muted.load(Ordering::Relaxed)
}

/// Plays the soft jelly plop for a Voronoi mascot mode cell.
pub fn play_mode(mode: InputMode) {
    handle().with_synth(|s| s.enqueue_mode(mode));
}

/// Plays the 4-note unfolding start chord.
pub fn play_start() {
    handle().with_synth(|s| s.enqueue_start());
}

/// Plays the directional chapter transition chime.
pub fn play_step(mode: InputMode, direction: i8) {
    handle().with_synth(|s| s.enqueue_step(mode, direction));
}

/// Plays the tactile option selection cue.
pub fn play_select(positive: bool) {
    handle().with_synth(|s| s.enqueue_select(positive));
}

/// Plays the climbing pentatonic pluck when pinning or unpinning an Essential app.
pub fn play_app_pin(pinned: bool, count: usize) {
    handle().with_synth(|s| s.enqueue_app_pin(pinned, count));
}

/// Plays the subtle keycap tap synced to the live Vim preview beats.
pub fn play_preview_key(mode: InputMode) {
    handle().with_synth(|s| s.enqueue_preview_key(mode));
}

/// Plays the crystalline shield deflection ping.
pub fn play_deflect() {
    handle().with_synth(|s| s.enqueue_deflect());
}

/// Plays the warm resolved Lydian finale chord.
pub fn play_finish() {
    handle().with_synth(|s| s.enqueue_finish());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthesizer_renders_bounded_nonzero_audio_for_every_cue() {
        let mut synth = SynthState::new(48_000.0, false);
        synth.enqueue_start();
        synth.enqueue_mode(InputMode::Explorer);
        synth.enqueue_step(InputMode::Toolbar, 1);
        synth.enqueue_select(true);
        synth.enqueue_app_pin(true, 3);
        synth.enqueue_preview_key(InputMode::Normal);
        synth.enqueue_deflect();
        synth.enqueue_finish();

        let mut buf = vec![0.0_f32; 4096];
        synth.render_mono(&mut buf);
        let peak = buf.iter().map(|s| s.abs()).fold(0.0_f32, f32::max);
        assert!(peak > 0.01, "expected audible signal, got peak {peak}");
        assert!(peak <= 0.95, "expected clipped peak <= 0.95, got {peak}");

        // Muting ramps smoothly down over 20 ms (~960 samples at 48 kHz), then stays silent.
        synth.set_muted(true);
        let mut ramp_buf = vec![0.0_f32; 1200];
        synth.render_mono(&mut ramp_buf);
        let tail = &ramp_buf[1050..];
        assert!(tail.iter().all(|&s| s == 0.0));
        let mut silent_buf = vec![0.0_f32; 512];
        synth.render_mono(&mut silent_buf);
        assert!(silent_buf.iter().all(|&s| s == 0.0));
    }
}
