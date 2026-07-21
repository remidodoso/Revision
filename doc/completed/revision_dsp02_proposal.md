# dsp-02 proposal — the PADsynth bake

**Status: approved and implemented 2026-07-21** (all thirteen decisions). Findings from implementation are in §13, and one of them corrects §4.2.

**Original status: proposed 2026-07-21.** Checkpoint per getstarted rule 2: a new
subsystem (`rev-dsp`), two new dependencies, a change to `rev-engine`'s public
API, and one new requirement.

Its input is `doc/revision_padlington_inventory.md` §2–§7, the read-only census
of the bake as it exists in Notorolla. The bake is pure data-in/data-out,
seeded, and headless — no engine, no device, no real-time constraint — which is
why it can be built and certified entirely on its own.

**The standard is not "correct."** It is Notorolla's Padlington, which is clean
across the whole keyboard in a way that complex digital synthesis usually is
not. This proposal's job is to reproduce that and say *why* it happens, so that
the cleanliness is a property of the construction rather than a coincidence
nobody wrote down.

---

## 1. Scope

**In.** The bake: profile, formants, partial placement, Gaussian smear, air
blend, seeded phases, IFFT, normalization (§3). Table geometry — how many
tables, at what base frequencies, band-limited how (§4). The determinism
discipline (§6). Where the level constants live (§7). The one `rev-engine`
change the geometry forces (§8). Tests: analytic identities, the aliasing
sweep, and a spectral helper so any crate can assert *pitch* (§9).

**Out — deferred, shape not foreclosed.** Hot-swapping a table into a running
engine (§4.4 removes the need for now). The LRU table cache. Per-region bakes
for top-octave stretch colour. Patch editing, presets, serialization. Any
second instrument.

**Out entirely.** Certification against exported Notorolla spectra — that is
dsp-03, where "certified" is the operative word (§10).

---

## 2. Where it lives

A new crate, `rev-dsp`, at `src/rust/dsp/`.

It depends on `rustfft`, `realfft`, and `thiserror` — **and on no workspace
crate at all.** It does not depend on `rev-engine` (which must not acquire an
FFT), and it does not depend on `rev-core` (the bake is physics, not music: it
takes frequencies and returns samples, R-312).

Its public surface is one function and two plain structs:

```rust
pub fn bake(spec: &BakeSpec, base_hz: f64, sample_rate: u32, len: usize) -> Vec<f32>;
pub fn bake_set(spec: &BakeSpec, anchor_hz: f64, sample_rate: u32) -> Vec<(f32, Vec<f32>)>;
```

`BakeSpec` is the twelve bake-relevant fields of the inventory's §2, `Hash + Eq`
so that it is a cache key the day there is a cache. The app builds
`rev_engine::Table` from the returned samples; `rev-dsp` has never heard of
`Table`.

---

## 3. The bake pipeline

Transcribed from inventory §3, which recorded the formulas rather than
paraphrasing them. All arithmetic in `f64`; only the finished table is `f32`.

1. **Profile.** `A_k = source_raw(k) × formant_mask(k·f0)`.
   - pulse: `|sin(πkd)|/k`, duty `d = 0.5 − shape·0.47`
   - saw→triangle: `1/k` at shape 0, else `|sin(πks)|/k²`, `s = shape·0.5`
   - voice: `1/k^1.1`; tilt: `1/k^e`
   - `formant_mask(f) = Σ gain_i · resonator(f, F_i·size, Q)` over three
     formants, gains `[1.0, 0.6, 0.4]`,
     `resonator = 1/√(1 + Q²(f/fc − fc/f)²)`; `none` = 1.0
2. **Placement.** `f_k = f0 · k^(1+stretch)` — the Sethares hook, kept.
3. **Smear.** `bw_hz = (2^(bandwidth/1200) − 1)·f0·k^bwScale`, floored to one
   bin; `σ = bw_hz/2.355`; amplitude `A_k/√σ` (**energy-constant per
   harmonic**, so Bandwidth changes lushness and never tilt); accumulated over
   `±4·bw_hz`.
4. **Air.** Pink `1/√f` × one-pole HP `r/√(1+r²)` × the same formant mask,
   crossfaded `(1−noise)·tone + noise·air` with air energy matched to tone.
5. **Phase and transform.** Seeded uniform phase per bin, Hermitian symmetry,
   one inverse real FFT (`realfft`), RMS-normalized to `PAD_TABLE_RMS`.

**One formant table**, owned here. The inventory found `padsynth.js` and
`audio.js` keeping two copies in hand-sync; the port collapses them.

**The bin floor matters more than it looks.** Table bins are `SR/len` apart —
0.366 Hz at 2^17. A partial does not land on a bin, but the smear *deposits
energy around* its true frequency, so the perceived pitch is the centroid and
is exact. At zero bandwidth that would collapse to bin quantization; hence
step 3's floor of one bin. This is what makes the bake compatible with a
continuous logarithmic frequency axis (R-943) without doing anything special:
coarse pitch comes from the base, fine pitch from a continuous playback rate.

---

## 4. Table geometry — the part that decides whether it sounds clean

### 4.1 What actually goes wrong

A table baked at `base` and read at rate `r` shifts every partial by `r`.
Anything crossing Nyquist folds back to `SR − r·f`, so the **lowest frequency
any fold can reach** is

```
fold_min = SR − r_max·(SR/2) = SR·(1 − r_max/2)
```

At octave spacing, `r_max = √2` and `fold_min ≈ 14.1 kHz` — inside ordinary
adult hearing, not merely inside exceptional hearing. Octave spacing was never
adequate.

### 4.2 Why Notorolla is nevertheless clean

Four mechanisms, none of them designed for this, each worth real attenuation.
They are recorded because **preserving them is now a requirement, and one of
them is easy to destroy by accident**:

1. **The series usually ends before Nyquist.** With `harmonics = 64`, `64·base`
   exceeds Nyquist only above `base ≈ 375 Hz`. Below that nothing sits near
   Nyquist and nothing folds at all.
2. **The smear is clipped at the array edge**, so the top of the spectrum rolls
   off rather than ending on a cliff — a few dB, exactly where it counts.
3. **Linear interpolation is a lowpass.** The triangle kernel applies `sinc²`:
   about −8 dB on a near-Nyquist partial and −27 dB on the reconstruction
   images. `engine/src/table.rs` justifies linear interpolation as *sufficient*
   given the rate window; it is in fact **load-bearing**, and "upgrading" to
   cubic or sinc interpolation would remove a filter and make the instrument
   dirtier. To be commented as such in the code.
4. **The key-tracked lowpass** sits near 14–15 kHz for the notes that can fold.

Stacked, a −18 dB top partial arrives 30–40 dB down. That is the missing margin,
and it is why the instrument sounds the way it does. It is also entirely
circumstantial: four coincidences that happen to align, with a margin nobody has
measured.

### 4.3 Why "inaudible" is the wrong target

Ultrasonic content is not harmless. Every reproduction chain is nonlinear
somewhere — amplifier, tweeter, DAC output stage, the cochlea itself — and a
nonlinearity translates content downward: two components at 19.6 and 21.1 kHz
produce an audible 1.5 kHz difference tone.

The asymmetry that makes this tractable:

> **Harmonic ultrasonic content is benign under nonlinearity; inharmonic
> content is not.** Harmonics *k* and *k+1* differ by exactly f₀, so their
> difference tone is itself a harmonic and fuses with the note. Fold products
> are inharmonic, so their difference tones are inharmonic too.

The special thing about aliasing was never that it is high. It is that it is
inharmonic, and the reproduction chain is a machine for moving inharmonicity
into the range where the ear is most critical. PADsynth is the worst case for
this, because Gaussian smearing means dense clusters rather than isolated
partials, and dense inharmonic content intermodulates into a fog rather than a
tone.

So the target is not "fold below the threshold of hearing." It is **no
inharmonic energy at any frequency, measured to Nyquist** (R-720, §11).

### 4.4 The geometry

**Half-octave spacing.** Bases at `anchor · 2^(n/2)` across the playable range,
so `r ∈ [2^-¼, 2^¼] = [0.841, 1.189]`.

**Band-limited to `Nyquist / r_max` = 20.2 kHz at 48 kHz**, not to Nyquist. No
partial can exceed Nyquist at any rate the table is ever read at, so fold energy
is **exactly zero by construction** — not attenuated, not relocated, absent.

The two decisions are complements: the spacing is what makes the strict limit
free. At octave spacing the cut would land at 17 kHz and would take real
harmonic content that some people hear. At half-octave it lands at 20.2 kHz,
past any documented hearing, and what it removes above that would have been
benign anyway.

The four mechanisms of §4.2 all remain in place. This is added margin, not a
replacement for them.

**The anchor is the patch's `reference_hz`**, the field already destined to
become the tuning's anchor frequency. `base = anchor · 2^(round(2·log₂(f/anchor))/2)`
— pure log₂, no octave-of-C, no 12-ET residue.

**Cost.** 16 tables of 2^17 `f32` = **8 MB**, baked eagerly at
`Instrument::new` on the app thread, with the measured time logged. Eager
baking is what keeps table hot-swap out of scope: the instrument arrives
complete, so nothing ever has to reach a running engine mid-flight.

**Rejected: internal oversampling.** Running the graph at 2× the device rate
would also delete the fold — the decimation filter removes it — but it costs
2× the DSP for every voice, adds a filter and ~0.5 ms of latency to the
real-time path, and doubles table memory regardless. A changed comparison
operator achieves the same result exactly. Oversampling becomes necessary when
a nonlinearity exists *inside* the engine (saturation, waveshaping, FM), where
band-limiting cannot help; then it should be applied per-node, not globally.

---

## 5. Sample rate

Tables are baked against the device's sample rate and re-baked if it changes;
`base_hz` and the band limit are both rate-relative, so nothing else moves.

`rev-engine`'s format request should **prefer a higher device rate when one is
offered** (HDMI carries up to 192 kHz). It costs us nothing and every margin in
§4 improves. It is a preference, not a dependency: the design must be correct
at 44.1 kHz.

---

## 6. Determinism

`rev-dsp` owns a named seeded PRNG. The bake seed is a hash of `BakeSpec` plus
base and rate, so the same patch always produces the same table, and no two
patches share phases by accident.

**Not shared with `rev-engine`.** The voice's seeded head offsets stay as they
are: deduplicating six lines would mean the real-time crate importing a
non-audio dependency, and its independence is worth more.

**Not bit-compatible with the JS.** Matching would mean reproducing mulberry32,
djb2 and a specific radix-2 FFT. The golden-master plan is phase-independent by
design (inventory §7), so the comparison basis is magnitude spectra and meters.
The Rust-side gate is R-1402: bake twice, compare bits.

---

## 7. Levels

`PAD_TABLE_RMS = 0.25` moves into `rev-dsp` — it is a property of the bake,
which is what makes Source, Harmonics and Bandwidth change colour and never
loudness (R-713). `VOICE_PEAK` and `PAD_NORM` stay with the instrument, since
they are play-time. One home each, no constant in two places.

---

## 8. The `rev-engine` change this forces

`NodeKind::BufferSource { table: TableId }` fixes the table when the graph is
*described*. Sixteen tables per instrument means the table is chosen **per
note**, from the note's frequency.

Proposed: the voice carries a `TableId` override, set at `note_on` alongside
frequency and duration; `BufferSource` reads the voice's override when present
and its spec's `TableId` otherwise. No allocation, no graph rebuild, no change
to `GraphSpec`. **Public API checkpoint** — the smallest change that supports
the geometry, and it leaves the described graph honest about what it is.

---

## 9. Tests

**Analytic identities**, ported from `notch/padsynth.mjs` rather than captured
as data — they are stronger than a fixture and they do not rot: saw = 1/k;
pulse at shape 0 = odd harmonics only, 1/k; saw at shape 1 = odd only, 1/k²;
tilt = 1/k^e; formant bypass, peak placement, and universality; stretch
placement; bandwidth widens without retilting; RMS normalization; finiteness;
bake determinism.

**The aliasing sweep — the test this proposal exists for.** Render every note
across the playable range, FFT each, and measure energy at frequencies that are
not multiples of the fundamental, **across the whole output band up to
Nyquist**. Assert a floor at every note. This turns "no audible aliasing" from
a property of one build into a property no build can lose. It also settles the
spacing empirically: run it at octave, half-octave and third-octave spacing and
keep the cheapest that clears the floor with room to spare — replacing the
estimates in §4 with measurements.

**A spectral helper in `rev-testkit`** (`spectrum`, FFT as a dev-dependency, so
it never ships): given a rendered buffer, return its fundamental and its
partials, so any crate can assert *"this note's fundamental is 440 Hz ± 1."*

That last one is a debt, and it should be stated plainly. In eng-07 an entire
run played at the wrong pitch with 331 tests green — bit-identity passes when
wrong is reproducible, onsets passed because timing was genuinely fine, and
nothing we had written listened to pitch. The defects that mattered were heard,
not measured. This helper is the smallest thing that closes that class.

---

## 10. Certification (dsp-03, stated here so the fixtures get collected)

The specification is comparative, not a constant: **Revision's inharmonic-energy
floor must be no worse than Notorolla's, at every note.** That is a stronger and
more honest statement than any threshold chosen in advance, and with §4.4 in
place the expectation is measurably better rather than equal.

Needed from Notorolla (read-only visit; nothing written there):

- the bake-relevant and play-time field values for **two patches** — a pad, and
  the pluck already in use, which stress opposite ends;
- a **chromatic reference render** across the full keyboard at 48 kHz;
- optionally, an exported magnitude spectrum of the default bake in dB.

---

## 11. Requirements

**New — R-720 [P1].** *Synthesis introduces no inharmonic energy anywhere in
the output band.* Conformance is measured to Nyquist, **not** to the audible
band: reproduction chains are nonlinear, and a nonlinearity translates
inharmonic ultrasonic content downward into the audible range as inharmonic
content, whereas harmonic ultrasonic content translates as harmonic and is
benign. "We cannot hear it" is not evidence of conformance; a measurement is.

**Touched.** R-705 (instruments ported as graph descriptions), R-706 (stochastic
elements seeded), R-713 (timbre parameters loudness-neutral — §7), R-943
(continuous logarithmic frequency axis — §3), R-1402 (deterministic renders).

---

## 12. Decisions to approve

1. New crate `rev-dsp` at `src/rust/dsp/`, depending on no workspace crate.
2. New dependencies: `rustfft`, `realfft` (MIT/Apache, no C, no GPL).
3. Public surface: `bake`, `bake_set`, `BakeSpec` — samples out, no `Table`.
4. Full bake in one step: four sources, shape, vowels/formants, air, stretch,
   bandwidth and bwScale.
5. **Half-octave table spacing**, 16 tables, 8 MB, baked eagerly at
   `Instrument::new`.
6. **Band limit at `Nyquist / r_max`** (20.2 kHz at 48 kHz), not at Nyquist.
7. Base anchor is the patch's `reference_hz`; bases at `anchor · 2^(n/2)`.
8. Linear interpolation in `table.rs` is load-bearing and stays, commented.
9. No internal oversampling; revisit per-node when a nonlinearity exists.
10. `rev-engine`: per-note `TableId` override set at `note_on`.
11. `PAD_TABLE_RMS` moves to `rev-dsp`; play-time levels stay with the
    instrument.
12. `rev-testkit` gains a `spectrum` helper (dev-only FFT).
13. New requirement **R-720**, and the aliasing sweep as its standing evidence.


---

## 13. Findings (written after the fact)

**1. The residue is not what §4 expected.** With the band limit in place, the
inharmonic energy left in a rendered note is neither fold-back nor the
interpolator: it is the **resampling images of a looped table**, at
`(r−1)·SR ± k·f0`. Identified by arithmetic rather than by suspicion — at
1568 Hz, read at 1.0595, the worst components measured at 9690, 11256 and
12823 Hz, and `(0.0595 × 48000) − k·1568` reproduces all three exactly. Worst
across the keyboard: **−27.6 dB at 2489 Hz**. The instrument being ported has
them too, and worse, because octave spacing reads at rates up to 1.414.
Removing them means reading *oversampled*, which is the per-node oversampling
conversation §4.4 deferred — now with a specific artifact attached to it rather
than a general anxiety.

**2. §4.2's third mechanism is only half right, and this corrects it.** "Linear
interpolation is load-bearing; upgrading it would make the instrument dirtier"
holds in the *source's* geometry, where content crosses Nyquist and linear's
`sinc²` droop attenuates it on the way. Under the strict band limit nothing
crosses Nyquist, so a better interpolator is weakly *better*. Measured rather
than argued: Catmull-Rom moved the residue by 2 dB, which is why it was not
adopted — it is not the dominant term.

**3. The measurement was wrong three times before it was right, each time in
the flattering direction.** A tolerance in cents alone, narrower than the
analysis window's main lobe, measured a mathematically pure harmonic series as
6 dB dirty. Peak-picking a PADsynth partial — a *noise band* whose every bin
carries an independent random phase — reported notes 16 to 32 cents flat.
And at a 2^15 table the one-bin smear floor quantized low-note pitch by 23
cents. All three looked exactly like instrument defects and all three were
measurement defects. The lesson is narrower than "measure": an instrument you
build to measure a thing is a thing that also has to be measured.

**4. §12.5's "a few hundred milliseconds" was pessimistic.** A full 2^17 bake is
**10 ms**; sixteen of them are 160 ms, and MHALL renders eight bars including
the whole bake in 0.64 s.

**5. The comparison test in §9 was not written.** The data does not support it:
half-octave versus octave spacing is a wash at this measurement's resolution
except at the top two notes, because both are dominated by finding 1. Claiming
otherwise would have been a test asserting a conclusion the numbers do not
reach. The band limit's effect is proven instead where it is unambiguous — on
the table's own spectrum, in `a_baked_table_has_no_energy_above_the_band_limit`.
