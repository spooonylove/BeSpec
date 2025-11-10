# Testing FFT Performance

## FFT Performance Test Results

| Test | FFT Size | Bar Count | Channel | Total Frames | Avg Time | Min Time | Max Time | FPS Potential | CPU Usage (60fps) |
|------|----------|-----------|---------|--------------|----------|----------|----------|---------------|-------------------|
| 1    | 1024     | 64        | Mono    | 3000         | 125.9µs  | 86.8µs   | 1.45ms   | 8000.0        | 0.8%              |
| 2    | 1024     | 256       | Mono    | 3000         | 138.904µs| 97.7µs   | 2.8925ms | 7246.4        | 0.8%              |
| 3    | 2048     |512    | Mono     | 2999  |  254.14µs| 205.5µs  | 1.0574ms | 3937. | 1.6% |
| 4    | 4096     | 512   | Mono     | 2999 | 524.576µs| 378.9µs  | 10.7881ms| 1908.4 | 3.3%|

Tested configurations:
- 64 bars, 1024 FFT:   125.9µs (0.8% budget) - baseline
- 256 bars, 1024 FFT:  138.9µs (0.9% budget) - minimal impact
- 512 bars, 2048 FFT:  254.1µs (1.6% budget) - excellent
- 512 bars, 4096 FFT:  524.6µs (3.3% budget) - still fast

Key findings:
- Bar count has negligible performance impact
- FFT size scales linearly (4x size = 4x time)
- Even 4096 FFT leaves 96.7% CPU for UI rendering
- Maximum tested spike: 10.79ms (still under frame budget)

Conclusion: Audio processing will NOT be a bottleneck.
Default configuration: 1024 FFT, 64 bars (optimal balance).

# FFT Latency: The Physics of Real-Time Audio

## What FFT Latency Actually Means

### The Core Concept

**FFT latency = How much audio data you need before you can compute the FFT**
```
FFT Size = 1024 samples
Sample Rate = 48,000 samples/second

Latency = 1024 samples ÷ 48,000 samples/sec = 0.0213 seconds = 21.3 ms
```

---

## Why This Latency Exists

### The FFT Needs a Complete "Window"
```
Audio stream (continuous):
[s₀][s₁][s₂][s₃][s₄][s₅][s₆]...[s₁₀₂₃][s₁₀₂₄]...
 ↓
 Wait... collecting samples...
 ↓
[1024 samples collected!]
 ↓
Now we can run FFT
 ↓
Result: Frequency spectrum
```

**You can't compute FFT until you have all 1024 samples.**

---

## Real-World Example

Imagine you're playing music:
```
Time 0ms:     Speaker plays first audio sample
Time 21.3ms:  We've collected 1024 samples
              ↓
              NOW we can compute FFT
              ↓
              NOW we can draw bars
              ↓
Time 22ms:    Bars appear on screen

Total delay: 22ms from sound → visual
```

**This is why it's called "latency"** - the delay between audio happening and you seeing it.

---

## Why Different FFT Sizes Have Different Latencies
```
┌─────────────┬────────────────┬─────────────┐
│ FFT Size    │ Samples Needed │ Latency     │
├─────────────┼────────────────┼─────────────┤
│ 512         │ 512 samples    │ 10.7 ms     │
│ 1024        │ 1024 samples   │ 21.3 ms     │
│ 2048        │ 2048 samples   │ 42.7 ms     │
│ 4096        │ 4096 samples   │ 85.3 ms     │
│ 8192        │ 8192 samples   │ 170.7 ms    │
└─────────────┴────────────────┴─────────────┘
```

**Formula:**
```
Latency (seconds) = FFT_Size / Sample_Rate
Latency (ms) = (FFT_Size / Sample_Rate) × 1000
```

---

## Why This Matters for BeAnal

### Human Perception Thresholds
```
< 20ms:   Feels instant (audio and visual perfectly synced)
20-50ms:  Acceptable (slight lag, barely noticeable)
50-100ms: Noticeable delay (feels slightly "off")
> 100ms:  Very noticeable (audio and visual clearly out of sync)
```

**Test results:**
- 1024 FFT = 21.3ms ✅ Feels instant
- 2048 FFT = 42.7ms ✅ Acceptable
- 4096 FFT = 85.3ms ⚠️ Starts to feel laggy
- 8192 FFT = 170.7ms ❌ Noticeably out of sync

---

## The Trade-off: Latency vs Resolution

### Smaller FFT (512)
```
✅ Low latency (10.7ms - feels instant)
✅ Fast computation (~50µs)
❌ Poor frequency resolution (93.8 Hz/bin)
❌ Can't distinguish nearby notes
```

**Good for:** Beat detection, rhythm games, DJ software

### Medium FFT (1024)
```
✅ Good latency (21.3ms - acceptable)
✅ Fast computation (~125µs)
✅ Decent resolution (46.9 Hz/bin)
✅ Good for music visualization
```

**Good for:** General music visualizers (your sweet spot!)

### Large FFT (2048)
```
⚠️ Noticeable latency (42.7ms)
✅ Still fast computation (~254µs)
✅ Good resolution (23.4 Hz/bin)
✅ Better bass detail
```

**Good for:** Detailed analysis, studio tools

### Very Large FFT (4096+)
```
❌ High latency (85ms+)
⚠️ Slower computation (~500µs+)
✅ Excellent resolution (11.7 Hz/bin)
✅ Can identify individual notes
```

**Good for:** Pitch detection, tuning apps, offline analysis

---

## Why The Test Checks This
```rust
#[test]
fn test_fft_latency() {
    let mut config = AppConfig::default();
    config.fft_size = 2048;
    assert!((config.fft_latency_ms() - 42.667).abs() < 0.01);
}
```

**What this catches:**

### Bug #1: Wrong Formula
```rust
// WRONG:
pub fn fft_latency_ms(&self) -> f32 {
    (self.fft_size as f32 * 48000.0) / 1000.0  // ← Multiply instead of divide!
}

// Test fails: expected 42.7ms, got 98,304ms
```

### Bug #2: Forgot to Convert to Milliseconds
```rust
// WRONG:
pub fn fft_latency_ms(&self) -> f32 {
    self.fft_size as f32 / 48000.0  // ← Returns seconds, not ms!
}

// Test fails: expected 42.667ms, got 0.042667ms
```

### Bug #3: Hardcoded Sample Rate
```rust
// WRONG:
pub fn fft_latency_ms(&self) -> f32 {
    (self.fft_size as f32 / 44100.0) * 1000.0  // ← Wrong sample rate!
}

// Test fails: expected 42.667ms, got 46.439ms
```

---

## The Physics Behind It

### Why Can't We Cheat?

**Q:** Can't we just start computing FFT before we have all samples?

**A:** No! FFT is a **global algorithm** - it needs ALL samples to compute frequencies.

Think of it like this:
```
Trying to identify a musical note:

With 100ms of audio:  "Definitely a C note"
With 50ms of audio:   "Probably a C... maybe C# ?"
With 10ms of audio:   "Could be C, D, E, or F..."
With 1ms of audio:    "No idea - not enough information"
```

**The Heisenberg Uncertainty Principle of Audio:**
```
Δt × Δf ≥ 1

Time resolution × Frequency resolution ≥ constant
```

You can't have **both** high time resolution (low latency) **and** high frequency resolution (detailed spectrum) at the same time!

---

## Real-World Analogy

Imagine trying to identify someone's handwriting:
```
1 letter:     Can't identify
1 word:       Maybe can identify
1 sentence:   Probably can identify
1 paragraph:  Definitely can identify
```

**More samples = better identification, but you have to wait longer to collect them.**

Same with FFT:
- 512 samples = rough idea of frequencies
- 1024 samples = good idea
- 4096 samples = precise identification

---

## Why We Display This in the UI
```
[FFT] Performance: Avg: 125.9µs | Min: 86.8µs | Max: 1.45ms
                    ^^^^^^^^^^
                    Processing time

[Stats Overlay] 1024 FFT | 46.9 Hz/bin | 21.3ms latency
                                         ^^^^^^^^^^^^^^^
                                         This is intrinsic delay
```

**Users should know:**
- Higher FFT size = better detail BUT more delay
- If bars feel "laggy," reduce FFT size
- If bass looks "mushy," increase FFT size

---

## The Specific Test Case
```rust
config.fft_size = 2048;
assert!((config.fft_latency_ms() - 42.667).abs() < 0.01);
//                                  ^^^^^^
//                         This is PHYSICS, not arbitrary!
```

**That 42.667ms is not a magic number - it's calculated from:**
```
2048 samples ÷ 48,000 samples/sec × 1000 = 42.666... ms
```

**The test verifies:**
- ✅ Formula is correct
- ✅ Math doesn't overflow
- ✅ Units are right (milliseconds, not seconds)

---

## Summary

**FFT latency is intrinsic because:**
1. FFT **requires** a complete window of samples
2. Collecting samples **takes time**
3. Larger window = better frequency resolution BUT more delay
4. This is **fundamental physics**, not a software limitation

**The test ensures:**
- Your latency calculation matches reality
- Users get accurate latency numbers in the UI
- You can make informed trade-offs (detail vs responsiveness)

---

## Key Takeaway

This is one of those beautiful moments where **math, physics, and user experience** all intersect. The latency isn't a bug or limitation of your code - it's an intrinsic property of how Fourier transforms work in the physical universe. Understanding this helps you make better design decisions about which FFT size to offer users and how to communicate the trade-offs in your UI.