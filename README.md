
## File Structure
```
src/
├── main.rs                  # Entry point, thread spawning
├── fft_processor.rs         # FFT processing (existing)
├── shared_state.rs          # SharedState, AppConfig, etc
├── gui/
│   ├── mod.rs              # GUI module
│   ├── app.rs              # BeAnalApp impl
│   ├── spectrum.rs         # Spectrum bar rendering
│   ├── settings.rs         # Settings window + tabs
│   └── theme.rs            # Colors, fonts, styles
└── audio/
    ├── mod.rs              # Audio module
    └── capture.rs          # Audio capture thread