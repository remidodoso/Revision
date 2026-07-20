# Padlington inventory (dsp-01) — read-only Notorolla visit, 2026-07-19

Input to dsp-02 (spectral builder) and eng-02 (node API/menu). Sources read:
`src/js/audio/padsynth.js` (the pure bake), `audio.js` (`buildPadlingtonVoice`,
`scheduleFilterEnv`, level constants), `instrument.js` (defaults/params),
`notch/padsynth.mjs` (tests), `notch/meter-pad.mjs` (level rig). No Notorolla
changes.

## 1. Architecture confirmed: precompute-heavy, playback-trivial

PADsynth (Paul Nasca / ZynAddSubFX): a harmonic amplitude profile is smeared
into Gaussian frequency bands, every bin gets a seeded random phase, one IFFT
yields a long seamlessly-looping wavetable. The bake is **pure data-in/data-out,
seeded, headless-tested** — no Web Audio anywhere in it. The voice is "two read
heads over the table into filter + ADSR" (~7 nodes, cheapest in the roster).
This is exactly the split the port plan assumed.

## 2. Patch fields

**Bake-relevant** (any change re-bakes; all keyed into `padTableKey`):

| field | default | notes |
|---|---|---|
| source | 'saw' | enum: saw \| pulse \| voice \| tilt |
| shape | 0 | 0..1; saw→triangle morph / pulse duty 0.5→0.03 |
| tilt | 1.5 | exponent, tilt source only |
| harmonics | 64 | profile length n |
| vowel | 'none' | none \| ooh \| oh \| ah \| eh \| ee (bypass at none) |
| size | 1.0 | formant-centre scale (vocal tract size) |
| formantQ | 9 | formant resonator Q (9 = the old fixed Choir Q) |
| noise | 0 | 0..1 Air blend (energy-matched crossfade) |
| airCut | 30 | Air high-pass corner Hz (1-pole, Juno-60 style) |
| bandwidth | 25 | Gaussian smear in cents (the lushness) |
| bwScale | 1.0 | smear growth up the series (1 = constant cents) |
| stretch | 0 | ±0.05; partial k at f0·k^(1+s) — **the Sethares hook** |

**Play-time** (never re-bake; cache key excludes them): pitchAtk ±200¢,
pitchAtkTime 0.08 s, width 0.7, cutoff 7000 Hz, reso 0.5, filterEnv (octaves),
keyTrack 0.3, attack 0.4 s, decay 1.0 s, sustain 0.9, release 1.2 s.

`padTableKey(p, baseFreq, sampleRate, tableSize)`: string of rounded
bake-relevant params — the cache key AND the bake seed (djb2 hash → mulberry32).

## 3. Bake pipeline (formulas)

1. **Profile** A1..An = sourceRaw(k) × formantMask(k·f0):
   - pulse: |sin(π·k·d)|/k, duty d = 0.5 − shape·(0.5−0.03); d=0.5 = odd-only 1/k square
   - saw→tri: 1/k at shape 0, else |sin(π·k·s)|/k² with s = shape·0.5 (s=0.5 = triangle)
   - voice: 1/k^1.1 (glottal); tilt: 1/k^e
   - formantMask(f) = Σᵢ gainᵢ · resonatorMag(f, Fᵢ·size, Q); 3 formants per vowel,
     gains [1.0, 0.6, 0.4]; resonatorMag = 1/√(1+Q²(f/fc−fc/f)²); 'none' = 1.0
2. **Partial placement**: f_k = f0 · k^(1+stretch); band-limited at Nyquist.
3. **Gaussian smear**: bwHz = (2^(bandwidth/1200)−1) · f0 · k^bwScale, floored to
   one bin; σ = bwHz/2.355 (FWHM→σ); amplitude scale = A/√σ (**energy-constant
   per harmonic** — else the Bandwidth knob retilts the spectrum); accumulated
   over ±4·bwHz around f_k.
4. **Air**: pink 1/√f × one-pole HP r/√(1+r²) (r=f/airCut) × the same formant
   mask; blended (1−noise)·tone + noise·scaledAir with air energy matched to
   tone energy.
5. **Phases + IFFT**: seeded uniform phase per bin (mulberry32(djb2(key)));
   Hermitian symmetry; in-place radix-2 complex FFT (inverse, unscaled);
   RMS-normalize to PAD_TABLE_RMS = 0.25.
6. **Table**: 2^17 samples (~2.7 s @ 48 kHz); bake ≈ tens of ms; lazy per
   (patch, octave-base), LRU cache of 16 per context.
7. **Base selection**: nearest octave-of-C to the note (C1..C8 clamp) —
   playbackRate = f0/base stays in ~[0.71, 1.41] (resampling artifacts
   negligible, formants anchored within a half octave).

## 4. Voice graph (the eng-02 node-menu census)

```
2 × [ BufferSource(loop, playbackRate=f0/base, detune) → Gain(1/√2) → StereoPanner(±width) ]
    → Gain(amp ADSR) → BiquadFilter(lowpass, Q=reso, freq automated) → dest
```

Two read heads with **random start offsets** (decorrelation → wide stereo from
one bake; per-note offsets are deliberately nondeterministic in JS — the
Revision port seeds them, per R-706 "more correct than its source").

**AudioParam vocabulary used** (= eng-04 module scope, exactly):
`setValueAtTime`, `linearRampToValueAtTime`, `exponentialRampToValueAtTime`,
`setTargetAtTime`.

**Envelope math** (duration known up front in JS's fire-and-forget; the Rust
voice gets real note_on/note_off, R-711 — the atkEnd clamping below is the
JS-ism that retires):
- Amp: linear attack 0→peak over `attack` (linear, NOT exponential — exp from
  near-zero is inaudible then snaps); a note shorter than the attack ramps only
  to peak·(dur/attack) and releases from there; decay = setTargetAtTime(τ=decay)
  toward peak·sustain; release = setTargetAtTime(τ=release) to 0.0001; source
  stop at release + max(0.5, 6·release).
- Filter (single-ADSR: tracks the amp envelope): baseCut = clamp(cutoff ·
  (f0/FREF)^keyTrack, 60, 0.45·SR); exp-ramp to baseCut·2^(filterEnv·attackFrac)
  (exp ramp = log-linear = the amp's linear attack in octave space); target
  baseCut·2^(filterEnv·sustain) τ=decay; release back to baseCut τ=release.
  FREF = middle C — in Revision this reference becomes tuning-aware.
- Pitch attack: detune = pitchAtk cents at start, setTargetAtTime(0,
  τ=pitchAtkTime/4) — signed (positive = approach from above).

## 5. Levels (one home — the law in action)

peak = velocity × VOICE_PEAK(0.095) × PAD_NORM(2.4). PAD_HEAD_GAIN = 1/√2
restores the table's baked RMS from two decorrelated heads. PAD_TABLE_RMS =
0.25 (bake-side normalization: Source/Harmonics/Bandwidth change color, never
loudness). PAD_NORM was set by metering against a default Vesperia note
(notch/meter-pad.mjs): sustained-pad peak ≈ ref −2 dB, RMS ≈ ref.

## 6. Test assets (golden-master material)

`notch/padsynth.mjs` (277 lines, 10 sections): FFT round-trip (1e-9); profile
identities (saw 1/k, pulse-at-0 odd-only square, saw-at-1 odd-only 1/k²
triangle, tilt 1/k^e, formant bypass/peaks/universality); stretch placement;
spectrum energy concentration + bandwidth widening; bake determinism +
RMS-normalization + finiteness; cache-key in/out partition; base selection;
registry plumbing; voice-graph shape; full-size default-bake smoke.
`notch/meter-pad.mjs`: peak/RMS metering rig vs the Vesperia reference (a
tuning rig, not pass/fail).

## 7. Port decisions surfaced (for dsp-02)

- **Comparison basis: magnitude spectra + meters, not bit-identity with JS.**
  Bit-compat would require reproducing mulberry32+djb2 and the exact radix-2
  FFT; the golden-master plan is phase-independent by design, so the Rust side
  uses its own core-owned seeded PRNG and rustfft/realfft, certified against
  exported JS spectra (dB tolerance) + notch-style meters. Rust-side gate:
  render twice → bit-identical (R-1402).
- **Seed the head offsets** (the one JS nondeterminism) — R-706 upgrade.
- **The formant table duplication** (padsynth.js PAD_VOWELS synced by hand with
  audio.js's Nayumi tables) collapses to one owned table in the port.
- **12-ET residue is negligible**: padBaseFreq's octave-of-C logic is pure
  log2-of-frequency (tuning-agnostic); the keyTrack FREF reference is the only
  12-ET-flavored constant, and becomes the tuning's anchor frequency.
- Table memory: 2^17 × f32 = 512 KB/table, LRU 16 ≈ 8 MB — same budget fine
  native; per-region bakes (top-octave stretch color) remain optional later.
