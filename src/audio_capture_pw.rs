//! Linux audio capture backend using native PipeWire (`pipewire-rs`).
//!
//! Replaces cpal/ALSA on Linux because cpal's "open the output device, call
//! `build_input_stream`" pattern is a Windows WASAPI loopback idiom that
//! silently misroutes on Linux ALSA — it ends up reading from the default
//! capture *source* (the system mic) instead of the requested sink's monitor.
//!
//! This backend creates a PipeWire input stream with `MEDIA_CATEGORY = Capture`
//! and `STREAM_CAPTURE_SINK = true`. With no explicit target node, PipeWire
//! auto-connects the stream to the *current default sink's monitor* and
//! transparently follows default-sink changes — exactly what an audio
//! visualizer wants. Format is negotiated to interleaved f32 stereo at the
//! sink's native rate.
//!
//! The PipeWire mainloop runs on its own thread; audio frames are forwarded
//! to the FFT thread via the same `crossbeam_channel<AudioPacket>` the cpal
//! backend uses, so the rest of bespec is unchanged.
//!
//! Implementation cribs from the `audio-capture` example in the upstream
//! `pipewire-rs` 0.9 crate, with two adaptations: STREAM_CAPTURE_SINK is set
//! by default (we want sink monitors, not mic input — that's bespec's job),
//! and the mainloop runs on a dedicated thread with a polling timer that
//! observes a shutdown atomic for graceful teardown from other threads.

use crossbeam_channel::{bounded, Receiver, Sender};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use pipewire as pw;
use pw::properties::properties;
use pw::spa;
use spa::param::audio::{AudioFormat, AudioInfoRaw};
use spa::param::format::{MediaSubtype, MediaType};
use spa::param::format_utils;
use spa::pod::{serialize::PodSerializer, Object, Pod, Value};

use crate::audio_capture::AudioPacket;
use crate::audio_device::{AudioDeviceError, AudioDeviceInfo};

/// Default rate / channel layout we request from PipeWire. PipeWire negotiates
/// down to whatever the default sink actually supports (typically 48000 Hz
/// stereo) and the actual values come back via the `param_changed` callback
/// before the first audio packet is delivered.
const DEFAULT_RATE: u32 = 48000;
const DEFAULT_CHANNELS: u32 = 2;

/// Bounded channel capacity between the realtime PipeWire process callback
/// and the FFT thread. Matches the cpal backend's `bounded(16)`.
const PACKET_QUEUE_SIZE: usize = 16;

/// How often the shutdown atomic is polled from inside the PipeWire mainloop.
const SHUTDOWN_POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Shared metadata between the audio thread (writer) and the manager
/// (reader). Atomics so there's no locking on the realtime path.
struct StreamMeta {
    sample_rate: AtomicU32,
    channels: AtomicU32,
}

/// Sentinel id for the "follow the current default sink monitor" virtual
/// device. This matches the existing cross-platform `"Default"` convention
/// hard-coded in `main.rs` and `gui/widgets.rs` so there is exactly one
/// "default device" identifier across all backends. Anything else is
/// interpreted as a literal PipeWire `node.name`.
pub const DEFAULT_DEVICE_ID: &str = "Default";

/// Linux audio capture manager. Public surface mirrors the cpal-based
/// `AudioCaptureManager` so `main.rs` and the GUI don't care which backend
/// is in use.
pub struct AudioCaptureManager {
    device_info: AudioDeviceInfo,
    /// Currently-selected device id. `DEFAULT_DEVICE_ID` means
    /// "auto-follow the default sink"; anything else is a PipeWire node
    /// name that the capture thread resolves at stream-creation time.
    selected_device: String,
    tx: Sender<AudioPacket>,
    rx: Receiver<AudioPacket>,
    shutdown: Arc<AtomicBool>,
    meta: Arc<StreamMeta>,
    capture_thread: Option<thread::JoinHandle<()>>,
}

impl AudioCaptureManager {
    /// Construct a manager for the system's default audio — the current
    /// default sink's monitor, auto-following any default-sink change.
    pub fn new() -> Result<Self, AudioDeviceError> {
        Self::with_info(default_device_info(), DEFAULT_DEVICE_ID.to_string())
    }

    /// Construct a manager for a specific device id. `id` is either the
    /// `DEFAULT_DEVICE_ID` sentinel or a literal PipeWire `node.name`
    /// (sink name for monitor capture, or source name for mic / line-in).
    pub fn with_device_id(device_id: &str) -> Result<Self, AudioDeviceError> {
        if device_id == DEFAULT_DEVICE_ID {
            Self::new()
        } else {
            Self::with_info(synthetic_info_for_id(device_id), device_id.to_string())
        }
    }

    fn with_info(
        device_info: AudioDeviceInfo,
        selected_device: impl Into<String>,
    ) -> Result<Self, AudioDeviceError> {
        let (tx, rx) = bounded(PACKET_QUEUE_SIZE);
        Ok(Self {
            device_info,
            selected_device: selected_device.into(),
            tx,
            rx,
            shutdown: Arc::new(AtomicBool::new(false)),
            meta: Arc::new(StreamMeta {
                sample_rate: AtomicU32::new(DEFAULT_RATE),
                channels: AtomicU32::new(DEFAULT_CHANNELS),
            }),
            capture_thread: None,
        })
    }

    /// Enumerate every audio source visible to PipeWire — both physical
    /// `Audio/Source` nodes (mics, line-ins) and `.monitor` sources of every
    /// `Audio/Sink` (i.e. one capture point per output device that mirrors
    /// what's playing on it).
    ///
    /// Does *not* include a synthetic "default" entry: the GUI dropdown in
    /// `gui/widgets.rs` already prepends a hard-coded "Default System Device"
    /// option for the cross-platform default-device sentinel, and adding our
    /// own would duplicate it. Returns an empty Vec on registry failure
    /// rather than an error so the GUI still gets to show the hard-coded
    /// default option even on a broken PipeWire daemon.
    pub fn list_devices() -> Result<Vec<AudioDeviceInfo>, AudioDeviceError> {
        match enumerate_pipewire_sources() {
            Ok(found) => Ok(found),
            Err(e) => {
                tracing::warn!(
                    "[AudioCapture] PipeWire registry enumeration failed: {} \
                     (GUI will fall back to the default-device option only)",
                    e
                );
                Ok(Vec::new())
            }
        }
    }

    /// Spawn the PipeWire mainloop on a worker thread and connect a capture
    /// stream. The thread resolves the selected device name against the live
    /// PipeWire registry and sets up the stream accordingly: a sink name
    /// becomes `TARGET_OBJECT` + `STREAM_CAPTURE_SINK = true`; a source name
    /// becomes plain `TARGET_OBJECT` (no capture-sink); and the
    /// `DEFAULT_DEVICE_ID` sentinel skips the lookup entirely so the stream
    /// auto-connects to the current default sink monitor.
    pub fn start_capture(&mut self) -> Result<(), AudioDeviceError> {
        let tx = self.tx.clone();
        let shutdown = Arc::clone(&self.shutdown);
        let meta = Arc::clone(&self.meta);
        let selected_device = self.selected_device.clone();

        tracing::info!(
            "[AudioCapture] Starting PipeWire capture: {}",
            selected_device,
        );

        let handle = thread::spawn(move || {
            if let Err(e) = run_pipewire_loop(tx, shutdown, meta, selected_device) {
                tracing::error!("[AudioCapture] PipeWire backend error: {}", e);
            }
        });

        self.capture_thread = Some(handle);
        Ok(())
    }

    /// Signal the PipeWire mainloop to quit and join the worker thread.
    pub fn stop_capture(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(handle) = self.capture_thread.take() {
            // Best-effort join. The polling timer inside the pipewire mainloop
            // sees the atomic flip and quits within ~SHUTDOWN_POLL_INTERVAL.
            let _ = handle.join();
        }
        // Reset for potential restart.
        self.shutdown.store(false, Ordering::Relaxed);
    }

    /// Switch the active device. Tears down the current capture stream
    /// (joining the worker thread) and starts a new one targeting the
    /// requested PipeWire node by name. The `DEFAULT_DEVICE_ID` sentinel
    /// switches back to "follow the default sink".
    pub fn switch_device(&mut self, device_id: &str) -> Result<(), AudioDeviceError> {
        self.stop_capture();
        self.selected_device = device_id.to_string();
        self.device_info = if device_id == DEFAULT_DEVICE_ID {
            default_device_info()
        } else {
            synthetic_info_for_id(device_id)
        };
        self.start_capture()
    }

    /// Receiver end of the audio packet channel. Cloned out to the FFT thread.
    pub fn receiver(&self) -> Receiver<AudioPacket> {
        self.rx.clone()
    }

    #[allow(dead_code)]
    pub fn device_info(&self) -> AudioDeviceInfo {
        let mut info = self.device_info.clone();
        // Surface the actual negotiated rate/channels so consumers see what
        // the stream is delivering rather than the defaults we requested.
        info.default_sample_rate = self.meta.sample_rate.load(Ordering::Relaxed);
        info.channels = self.meta.channels.load(Ordering::Relaxed) as u16;
        info
    }
}

impl Drop for AudioCaptureManager {
    fn drop(&mut self) {
        self.stop_capture();
    }
}

/// Build the synthetic "System Audio" device descriptor exposed to the GUI
/// picker. The numbers reflect the format we request from PipeWire; the real
/// negotiated values surface via `device_info()` after the stream connects.
fn default_device_info() -> AudioDeviceInfo {
    AudioDeviceInfo {
        id: DEFAULT_DEVICE_ID.to_string(),
        name: "System Audio (Default Output)".to_string(),
        sample_rates: vec![DEFAULT_RATE],
        default_sample_rate: DEFAULT_RATE,
        channels: DEFAULT_CHANNELS as u16,
        is_default: true,
    }
}

/// Build a placeholder `AudioDeviceInfo` for a device id we don't yet have
/// rich metadata for. Used by `with_device_id` / `switch_device` when the
/// caller restored a saved selection from disk before `list_devices()` was
/// called. The id is the literal PipeWire `node.name`; the registry walk
/// inside `run_pipewire_loop` resolves it to a real node at capture time.
fn synthetic_info_for_id(device_id: &str) -> AudioDeviceInfo {
    AudioDeviceInfo {
        id: device_id.to_string(),
        name: device_id.to_string(),
        sample_rates: vec![DEFAULT_RATE],
        default_sample_rate: DEFAULT_RATE,
        channels: DEFAULT_CHANNELS as u16,
        is_default: false,
    }
}

/// Per-capture-thread state. Both pipewire callbacks receive `&mut` to this
/// via the listener's user-data slot; the mainloop ensures they run serially
/// on the capture thread, so no extra synchronization is needed inside.
struct CaptureState {
    tx: Sender<AudioPacket>,
    meta: Arc<StreamMeta>,
    format: AudioInfoRaw,
    /// Set to true by `param_changed` once we've confirmed the negotiated
    /// format is interleaved F32LE (the only format we ask for + the only
    /// one `process` knows how to interpret). Until this is true, `process`
    /// drops every quantum so we never reinterpret bytes under an unknown
    /// sample type.
    format_validated: bool,
    started_logged: bool,
}

/// What kind of node `selected_device` resolves to in the live PipeWire
/// registry — drives whether we set `STREAM_CAPTURE_SINK` and whether we
/// pin a `TARGET_OBJECT`.
enum ResolvedTarget {
    /// `DEFAULT_DEVICE_ID` sentinel — let pipewire auto-connect to the
    /// current default sink monitor.
    DefaultSink,
    /// A specific Audio/Sink: target it by name with `STREAM_CAPTURE_SINK`
    /// to grab the monitor side.
    SinkMonitor { node_name: String },
    /// A specific Audio/Source (mic / line-in / loopback): target it
    /// by name without `STREAM_CAPTURE_SINK`.
    Source { node_name: String },
}

/// Run the PipeWire mainloop until `shutdown` is set. Owns the entire
/// pipewire-rs object graph (mainloop / context / core / stream / listener)
/// for the duration of the capture session.
fn run_pipewire_loop(
    tx: Sender<AudioPacket>,
    shutdown: Arc<AtomicBool>,
    meta: Arc<StreamMeta>,
    selected_device: impl Into<String>,
) -> Result<(), AudioDeviceError> {
    let selected_device = selected_device.into();
    pw::init();

    let mainloop = pw::main_loop::MainLoopRc::new(None)
        .map_err(|e| AudioDeviceError::PipeWireError(format!("MainLoopRc::new failed: {e}")))?;
    let context = pw::context::ContextRc::new(&mainloop, None)
        .map_err(|e| AudioDeviceError::PipeWireError(format!("ContextRc::new failed: {e}")))?;
    let core = context
        .connect_rc(None)
        .map_err(|e| AudioDeviceError::PipeWireError(format!("connect_rc failed: {e}")))?;

    // Resolve the saved device name against the live registry so we know
    // whether to capture a sink monitor or a plain source. Round-trips on
    // the same mainloop, ~10ms on a local pipewire socket.
    let target = if selected_device == DEFAULT_DEVICE_ID {
        ResolvedTarget::DefaultSink
    } else {
        match resolve_target(&mainloop, &core, &selected_device) {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(
                    "[AudioCapture] Could not resolve '{}' in PipeWire registry \
                     ({}); falling back to default sink monitor",
                    selected_device,
                    e
                );
                ResolvedTarget::DefaultSink
            }
        }
    };

    // Build stream properties based on the resolved target.
    let mut props = properties! {
        *pw::keys::MEDIA_TYPE => "Audio",
        *pw::keys::MEDIA_CATEGORY => "Capture",
        *pw::keys::MEDIA_ROLE => "Music",
        *pw::keys::NODE_NAME => "bespec-capture",
        *pw::keys::NODE_DESCRIPTION => "BeSpec Visualizer Capture",
    };
    match &target {
        ResolvedTarget::DefaultSink => {
            // No TARGET_OBJECT: pipewire auto-connects to the current default
            // sink and re-routes on default-sink changes.
            props.insert(*pw::keys::STREAM_CAPTURE_SINK, "true");
            tracing::info!("[AudioCapture] Targeting default sink monitor (auto-follow)");
        }
        ResolvedTarget::SinkMonitor { node_name } => {
            props.insert(*pw::keys::STREAM_CAPTURE_SINK, "true");
            props.insert(*pw::keys::TARGET_OBJECT, node_name.clone());
            tracing::info!("[AudioCapture] Targeting sink monitor: {}", node_name);
        }
        ResolvedTarget::Source { node_name } => {
            props.insert(*pw::keys::TARGET_OBJECT, node_name.clone());
            tracing::info!("[AudioCapture] Targeting input source: {}", node_name);
        }
    }

    let stream = pw::stream::StreamBox::new(&core, "bespec-capture", props)
        .map_err(|e| AudioDeviceError::PipeWireError(format!("StreamBox::new failed: {e}")))?;

    let user_data = CaptureState {
        tx,
        meta: Arc::clone(&meta),
        format: AudioInfoRaw::new(),
        format_validated: false,
        started_logged: false,
    };

    let _listener = stream
        .add_local_listener_with_user_data(user_data)
        .param_changed(|_stream, state, id, param| {
            // Only react to format negotiation events.
            let Some(param) = param else { return };
            if id != pw::spa::param::ParamType::Format.as_raw() {
                return;
            }

            // Reject anything that isn't raw audio.
            let (media_type, media_subtype) = match format_utils::parse_format(param) {
                Ok(v) => v,
                Err(_) => return,
            };
            if media_type != MediaType::Audio || media_subtype != MediaSubtype::Raw {
                return;
            }

            // Pull out rate/channels and publish them so the manager (and
            // therefore the FFT thread, via AudioPacket) sees the real values.
            if state.format.parse(param).is_ok() {
                // Only F32LE is safe for the `process` callback's typed
                // reinterpret — explicitly validate before unblocking it.
                if state.format.format() != AudioFormat::F32LE {
                    tracing::error!(
                        "[AudioCapture] PipeWire negotiated unsupported sample \
                         format {:?} (expected F32LE); refusing to deliver \
                         audio. This should not happen — bespec only requests \
                         F32LE in the format pod.",
                        state.format.format()
                    );
                    state.format_validated = false;
                    return;
                }
                state.format_validated = true;
                state
                    .meta
                    .sample_rate
                    .store(state.format.rate(), Ordering::Relaxed);
                state
                    .meta
                    .channels
                    .store(state.format.channels(), Ordering::Relaxed);
                tracing::info!(
                    "[AudioCapture] PipeWire format negotiated: {} Hz, {} ch (F32LE)",
                    state.format.rate(),
                    state.format.channels()
                );
            }
        })
        .process(|stream, state| {
            // Refuse to interpret bytes until param_changed has confirmed
            // the negotiated format is F32LE. Without this gate, a future
            // PipeWire change that lands on e.g. S16LE would corrupt the
            // FFT input (and casting bytes-as-f32 under an unknown sample
            // type is undefined behavior).
            if !state.format_validated {
                return;
            }

            // Pull one buffer out of the realtime queue. None happens
            // occasionally under load; the next quantum will catch up.
            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };

            let datas = buffer.datas_mut();
            if datas.is_empty() {
                return;
            }
            let data = &mut datas[0];

            let n_channels = state.format.channels();
            if n_channels == 0 {
                return;
            }

            let chunk_size = data.chunk().size() as usize;
            let n_samples = chunk_size / std::mem::size_of::<f32>();
            if n_samples == 0 {
                return;
            }

            let Some(bytes) = data.data() else { return };
            if bytes.len() < chunk_size {
                return;
            }

            // Cast the byte slice to `&[f32]` via `align_to`, which checks
            // alignment and length safely. PipeWire usually hands us a
            // properly aligned buffer (the SHM segment is page-aligned and
            // chunks start on f32 boundaries) but we can't *prove* that
            // statically, and an unchecked `from_raw_parts` cast on a
            // misaligned buffer is immediate UB. align_to on an aligned
            // buffer is zero-cost — it just returns the middle slice.
            let sample_bytes = &bytes[..chunk_size];
            // SAFETY: u8 has alignment 1, so reinterpreting any byte slice
            // as f32 via align_to is well-defined; align_to never produces
            // unaligned f32 elements. The middle slice is exactly the
            // longest f32 prefix of the byte buffer with the correct
            // alignment.
            let (prefix, samples, suffix) = unsafe { sample_bytes.align_to::<f32>() };
            if !prefix.is_empty() || !suffix.is_empty() || samples.len() != n_samples {
                tracing::warn!(
                    "[AudioCapture] dropping PipeWire quantum with incompatible \
                     f32 alignment/size (prefix={}, suffix={}, got={}, expected={})",
                    prefix.len(),
                    suffix.len(),
                    samples.len(),
                    n_samples
                );
                return;
            }

            if !state.started_logged {
                tracing::info!("[AudioCapture] ✓ PipeWire stream delivering audio");
                state.started_logged = true;
            }

            let packet = AudioPacket {
                samples: samples.to_vec(),
                sample_rate: state.format.rate(),
                channels: n_channels as u16,
                timestamp: Instant::now(),
            };

            // try_send: if the FFT thread is behind, drop this quantum rather
            // than block the realtime PipeWire callback. Matches the cpal
            // backend's behavior under sustained overload.
            let _ = state.tx.try_send(packet);
        })
        .register()
        .map_err(|e| AudioDeviceError::PipeWireError(format!("listener register failed: {e}")))?;

    // Format param: request f32 LE; leave rate/channels unset so PipeWire
    // negotiates to the default sink's native format. The actual values come
    // back via `param_changed` above.
    let mut audio_info = AudioInfoRaw::new();
    audio_info.set_format(AudioFormat::F32LE);

    let pod_obj = Object {
        type_: pw::spa::utils::SpaTypes::ObjectParamFormat.as_raw(),
        id: pw::spa::param::ParamType::EnumFormat.as_raw(),
        properties: audio_info.into(),
    };
    let pod_bytes: Vec<u8> = PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &Value::Object(pod_obj),
    )
    .map_err(|e| AudioDeviceError::PipeWireError(format!("pod serialize failed: {e}")))?
    .0
    .into_inner();

    let mut params = [Pod::from_bytes(&pod_bytes)
        .ok_or_else(|| {
            AudioDeviceError::PipeWireError(
                "failed to parse serialized format pod".to_string(),
            )
        })?];

    stream
        .connect(
            spa::utils::Direction::Input,
            None, // any node — autoconnect to the default sink monitor
            pw::stream::StreamFlags::AUTOCONNECT
                | pw::stream::StreamFlags::MAP_BUFFERS
                | pw::stream::StreamFlags::RT_PROCESS,
            &mut params,
        )
        .map_err(|e| AudioDeviceError::PipeWireError(format!("stream.connect failed: {e}")))?;

    // Polling timer for graceful shutdown: every ~50ms, check the shutdown
    // atomic and quit the mainloop when set. The closure runs on the same
    // thread as the mainloop, so calling .quit() from inside is safe.
    let mainloop_for_timer = mainloop.clone();
    let shutdown_for_timer = Arc::clone(&shutdown);
    let timer = mainloop.loop_().add_timer(move |_expirations| {
        if shutdown_for_timer.load(Ordering::Relaxed) {
            mainloop_for_timer.quit();
        }
    });
    timer
        .update_timer(Some(SHUTDOWN_POLL_INTERVAL), Some(SHUTDOWN_POLL_INTERVAL))
        .into_result()
        .map_err(|e| AudioDeviceError::PipeWireError(format!("update_timer failed: {e:?}")))?;

    mainloop.run();

    tracing::info!("[AudioCapture] PipeWire mainloop exited");
    Ok(())
}

// ===========================================================================
//  Registry walk: enumerate every Audio/Source the graph exposes
// ===========================================================================
//
// Stand up a short-lived pipewire connection, walk the registry once via the
// standard `core.sync` round-trip pattern, collect every node whose
// `media.class` is `Audio/Source` (covers both physical inputs *and* the
// `.monitor` source of every Audio/Sink, since pipewire publishes both as
// Audio/Source globals), turn each into an AudioDeviceInfo, then tear the
// connection down. Called from `list_devices()` and exposed via the
// `list-pw-sources` example so devs can inspect the discovered set without
// rebuilding bespec.

/// Which kind of PipeWire source a `PipewireSource` refers to.
///
/// Using an enum instead of a `bool is_monitor` field avoids "boolean
/// blindness" at call sites — e.g. the test helper reads
/// `src("...", "...", SourceType::Monitor)` instead of `src("...", "...",
/// true)` where the reader has to guess what `true` means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceType {
    /// The `.monitor` side of an Audio/Sink — captures what's *playing*
    /// on that sink. Drives picker grouping under "Outputs".
    Monitor,
    /// A physical Audio/Source (mic, line-in, virtual capture node).
    Input,
}

/// One Audio/Source node as we read it from the PipeWire registry. Kept
/// public to the crate so the standalone `list-pw-sources` example can
/// inspect the same data list_devices uses.
#[derive(Clone, Debug)]
pub struct PipewireSource {
    /// Stable node name — the string we pass back as `AudioDeviceInfo.id`
    /// and later as PipeWire `TARGET_OBJECT` when capturing this source.
    pub node_name: String,
    /// Human-readable description (`node.description`), or the node name if
    /// no description was set.
    pub description: String,
    /// Whether this source is a sink monitor (output-side capture) or a
    /// physical input. Drives picker grouping.
    pub source_type: SourceType,
}

impl PipewireSource {
    /// User-facing label for the GUI device picker. Disambiguates monitor
    /// sources (output capture) from physical inputs (mic / line-in).
    pub fn display_name(&self) -> String {
        let tag = match self.source_type {
            SourceType::Monitor => "Output Monitor",
            SourceType::Input => "Input",
        };
        format!("{} ({})", self.description, tag)
    }
}

/// Walk the PipeWire registry once and return every `Audio/Source` node.
///
/// Uses the round-trip pattern from `pipewire-rs/examples/roundtrip.rs`:
/// fire `core.sync(0)`, drive the mainloop until the matching `done` event
/// arrives, tear everything down. The whole thing typically completes in a
/// few milliseconds on a local pipewire socket.
pub fn enumerate_pipewire_sources() -> Result<Vec<AudioDeviceInfo>, AudioDeviceError> {
    use std::cell::RefCell;
    use std::rc::Rc;

    pw::init();

    let mainloop = pw::main_loop::MainLoopRc::new(None)
        .map_err(|e| AudioDeviceError::PipeWireError(format!("MainLoopRc::new failed: {e}")))?;
    let context = pw::context::ContextRc::new(&mainloop, None)
        .map_err(|e| AudioDeviceError::PipeWireError(format!("ContextRc::new failed: {e}")))?;
    let core = context
        .connect_rc(None)
        .map_err(|e| AudioDeviceError::PipeWireError(format!("connect_rc failed: {e}")))?;
    let registry = core
        .get_registry_rc()
        .map_err(|e| AudioDeviceError::PipeWireError(format!("get_registry_rc failed: {e}")))?;

    let sources: Rc<RefCell<Vec<PipewireSource>>> = Rc::new(RefCell::new(Vec::new()));
    let sources_for_global = Rc::clone(&sources);

    // Fire the sync request *before* installing the done listener so the
    // closures don't have to capture the seq number through a Cell.
    let pending = core
        .sync(0)
        .map_err(|e| AudioDeviceError::PipeWireError(format!("core.sync failed: {e}")))?;

    let done = Rc::new(std::cell::Cell::new(false));
    let done_for_listener = Rc::clone(&done);
    let mainloop_for_listener = mainloop.clone();

    let _core_listener = core
        .add_listener_local()
        .done(move |id, seq| {
            if id == pw::core::PW_ID_CORE && seq == pending {
                done_for_listener.set(true);
                mainloop_for_listener.quit();
            }
        })
        .register();

    let _registry_listener = registry
        .add_listener_local()
        .global(move |obj| {
            if obj.type_ != pw::types::ObjectType::Node {
                return;
            }
            let Some(props) = obj.props.as_ref() else {
                return;
            };
            let Some(media_class) = props.get("media.class") else {
                return;
            };
            let Some(node_name) = props.get("node.name") else {
                return;
            };
            let description = props
                .get("node.description")
                .map(|s| s.to_string())
                .unwrap_or_else(|| node_name.to_string());

            match media_class {
                // Physical inputs and any monitors that pipewire-pulse
                // happens to materialize as standalone source globals.
                "Audio/Source" => {
                    let source_type = if node_name.ends_with(".monitor") {
                        SourceType::Monitor
                    } else {
                        SourceType::Input
                    };
                    sources_for_global.borrow_mut().push(PipewireSource {
                        node_name: node_name.to_string(),
                        description,
                        source_type,
                    });
                }
                // Sinks: synthesize a monitor entry. PipeWire doesn't publish
                // monitors as standalone Audio/Source globals — the monitor
                // is an implicit aspect of the sink, reached by targeting the
                // sink's node name with STREAM_CAPTURE_SINK = true at capture
                // time. Storing the sink name (no `.monitor` suffix) lets us
                // pass it as TARGET_OBJECT directly later.
                "Audio/Sink" => {
                    sources_for_global.borrow_mut().push(PipewireSource {
                        node_name: node_name.to_string(),
                        description,
                        source_type: SourceType::Monitor,
                    });
                }
                _ => {}
            }
        })
        .register();

    // Drive the loop until the done callback flips the flag. The roundtrip
    // example wraps this in `while !done.get() { mainloop.run(); }` because
    // pipewire-rs may return early under some signal-handling conditions.
    while !done.get() {
        mainloop.run();
    }

    // Stable ordering — monitors first (visualizer-typical default), then
    // inputs, each section sorted by description so the picker is
    // predictable across runs.
    //
    // We clone out of the Rc<RefCell<>> rather than `try_unwrap`-ing it,
    // because the registry/core listener closures still hold their own clones
    // of the same Rc until they're dropped at end-of-function.
    let mut sources = sources.borrow().clone();
    sources.sort_by(|a, b| {
        // Monitors before Inputs: reverse-sort on the boolean-equivalent
        // "is this a monitor" — `SourceType` is ordered via derived Ord? No,
        // we only derive PartialEq/Eq, so compare explicitly.
        let a_is_mon = matches!(a.source_type, SourceType::Monitor);
        let b_is_mon = matches!(b.source_type, SourceType::Monitor);
        b_is_mon
            .cmp(&a_is_mon)
            .then_with(|| a.description.cmp(&b.description))
    });

    Ok(sources.into_iter().map(source_to_device_info).collect())
}

/// Convert a discovered `PipewireSource` into the cross-platform
/// `AudioDeviceInfo` shape the GUI device picker consumes.
fn source_to_device_info(source: PipewireSource) -> AudioDeviceInfo {
    AudioDeviceInfo {
        id: source.node_name.clone(),
        name: source.display_name(),
        sample_rates: vec![DEFAULT_RATE],
        default_sample_rate: DEFAULT_RATE,
        channels: DEFAULT_CHANNELS as u16,
        is_default: false,
    }
}

/// Walk the PipeWire registry on an existing core and figure out whether
/// `node_name` refers to an Audio/Sink (so we capture its monitor) or an
/// Audio/Source (so we capture it directly). Reuses the same mainloop the
/// caller is about to drive for capture, so this only adds one extra
/// roundtrip per session.
fn resolve_target(
    mainloop: &pw::main_loop::MainLoopRc,
    core: &pw::core::CoreRc,
    node_name: &str,
) -> Result<ResolvedTarget, AudioDeviceError> {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    let registry = core
        .get_registry_rc()
        .map_err(|e| AudioDeviceError::PipeWireError(format!("get_registry_rc failed: {e}")))?;

    // (is_sink, is_source) populated by the registry walk.
    let found: Rc<RefCell<Option<bool>>> = Rc::new(RefCell::new(None));
    let found_for_listener = Rc::clone(&found);
    let target_name = node_name.to_string();

    let pending = core
        .sync(0)
        .map_err(|e| AudioDeviceError::PipeWireError(format!("core.sync failed: {e}")))?;
    let done = Rc::new(Cell::new(false));
    let done_for_listener = Rc::clone(&done);
    let mainloop_for_listener = mainloop.clone();

    let _core_listener = core
        .add_listener_local()
        .done(move |id, seq| {
            if id == pw::core::PW_ID_CORE && seq == pending {
                done_for_listener.set(true);
                mainloop_for_listener.quit();
            }
        })
        .register();

    let _registry_listener = registry
        .add_listener_local()
        .global(move |obj| {
            if obj.type_ != pw::types::ObjectType::Node {
                return;
            }
            let Some(props) = obj.props.as_ref() else {
                return;
            };
            if props.get("node.name") != Some(target_name.as_str()) {
                return;
            }
            match props.get("media.class") {
                Some("Audio/Sink") => *found_for_listener.borrow_mut() = Some(true),
                Some("Audio/Source") => *found_for_listener.borrow_mut() = Some(false),
                _ => {}
            }
        })
        .register();

    while !done.get() {
        mainloop.run();
    }

    // Copy the value out of the RefCell into a local before the match so the
    // temporary `Ref` doesn't get held across the closing brace and trip
    // borrow-checker NLL.
    let result = *found.borrow();
    match result {
        Some(true) => Ok(ResolvedTarget::SinkMonitor {
            node_name: node_name.to_string(),
        }),
        Some(false) => Ok(ResolvedTarget::Source {
            node_name: node_name.to_string(),
        }),
        None => Err(AudioDeviceError::PipeWireError(format!(
            "node '{node_name}' not present in registry"
        ))),
    }
}

// ===========================================================================
//  Unit tests
// ===========================================================================
//
// These cover the pure data-shaping logic that doesn't need a live PipeWire
// daemon: display labelling, the conversion to AudioDeviceInfo, and the
// sort key used for the GUI dropdown ordering. The actual capture path is
// integration-tested by running bespec against a real session.

#[cfg(test)]
mod tests {
    use super::*;

    fn src(name: &str, desc: &str, source_type: SourceType) -> PipewireSource {
        PipewireSource {
            node_name: name.to_string(),
            description: desc.to_string(),
            source_type,
        }
    }

    #[test]
    fn display_name_tags_monitors_and_inputs_distinctly() {
        let mon = src("alsa_output.foo", "Foo Card", SourceType::Monitor);
        let inp = src("alsa_input.foo", "Foo Card", SourceType::Input);
        assert_eq!(mon.display_name(), "Foo Card (Output Monitor)");
        assert_eq!(inp.display_name(), "Foo Card (Input)");
    }

    #[test]
    fn source_to_device_info_preserves_node_name_as_id() {
        let s = src("alsa_output.bar.analog-stereo", "Bar Sink", SourceType::Monitor);
        let info = source_to_device_info(s);
        assert_eq!(info.id, "alsa_output.bar.analog-stereo");
        assert_eq!(info.name, "Bar Sink (Output Monitor)");
        assert!(!info.is_default);
        assert_eq!(info.channels, DEFAULT_CHANNELS as u16);
        assert_eq!(info.default_sample_rate, DEFAULT_RATE);
    }

    #[test]
    fn synthetic_info_for_id_round_trips_node_name() {
        let info = synthetic_info_for_id("alsa_output.usb-foo.analog-stereo");
        assert_eq!(info.id, "alsa_output.usb-foo.analog-stereo");
        assert_eq!(info.name, "alsa_output.usb-foo.analog-stereo");
        assert!(!info.is_default);
    }

    #[test]
    fn default_device_info_uses_sentinel_id() {
        let info = default_device_info();
        assert_eq!(info.id, DEFAULT_DEVICE_ID);
        assert!(info.is_default);
    }

    #[test]
    fn sources_sort_monitors_first_then_alphabetical() {
        // Mirrors the comparator in `enumerate_pipewire_sources`: monitors
        // before inputs, each section sorted by description.
        let mut entries = vec![
            src("alsa_input.b", "Bravo Mic", SourceType::Input),
            src("alsa_output.a", "Alpha Sink", SourceType::Monitor),
            src("alsa_input.a", "Alpha Mic", SourceType::Input),
            src("alsa_output.b", "Bravo Sink", SourceType::Monitor),
        ];
        entries.sort_by(|a, b| {
            let a_is_mon = matches!(a.source_type, SourceType::Monitor);
            let b_is_mon = matches!(b.source_type, SourceType::Monitor);
            b_is_mon
                .cmp(&a_is_mon)
                .then_with(|| a.description.cmp(&b.description))
        });
        let order: Vec<&str> = entries.iter().map(|s| s.description.as_str()).collect();
        assert_eq!(order, vec!["Alpha Sink", "Bravo Sink", "Alpha Mic", "Bravo Mic"]);
    }
}

