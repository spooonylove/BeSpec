//! Standalone diagnostic that lists every Audio/Source node visible to
//! PipeWire, classified into output monitors (i.e. "what's playing on this
//! sink") and physical inputs (mics, line-ins). Exists so the linux audio
//! enumeration can be tested without rebuilding bespec or launching the GUI.
//!
//! Run with:
//!
//!     cargo run --release --example list-pw-sources
//!
//! This is a self-contained reference implementation of the same registry
//! walk used by `audio_capture_pw::enumerate_pipewire_sources()`. Linux only —
//! on macOS / Windows the bespec audio backend uses cpal so this example
//! short-circuits to a friendly "linux only" message. The crate-level `main`
//! function still has to exist on every platform or cargo refuses to build
//! the example, hence the cfg-gated-body / stub-main split below.

#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use pipewire as pw;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum SourceType {
        Monitor,
        Input,
    }

    #[derive(Debug)]
    struct Source {
        node_name: String,
        description: String,
        source_type: SourceType,
    }

    pw::init();

    let mainloop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&mainloop, None)?;
    let core = context.connect_rc(None)?;
    let registry = core.get_registry_rc()?;

    let sources: Rc<RefCell<Vec<Source>>> = Rc::new(RefCell::new(Vec::new()));
    let sources_for_global = Rc::clone(&sources);

    // Standard pipewire-rs roundtrip pattern: fire `core.sync(0)` first so
    // the seq id can be moved into the done listener without a Cell, then
    // install the listeners and drive the loop until done.
    let pending = core.sync(0)?;
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
                // Physical inputs and any monitors that the pipewire-pulse
                // compat layer happens to materialize as standalone sources.
                "Audio/Source" => {
                    let source_type = if node_name.ends_with(".monitor") {
                        SourceType::Monitor
                    } else {
                        SourceType::Input
                    };
                    sources_for_global.borrow_mut().push(Source {
                        node_name: node_name.to_string(),
                        description,
                        source_type,
                    });
                }
                // Sinks: synthesize a monitor entry. PipeWire doesn't publish
                // monitors as standalone Audio/Source globals — the monitor
                // is an implicit aspect of the sink that you reach by
                // targeting the sink's node name with STREAM_CAPTURE_SINK.
                "Audio/Sink" => {
                    sources_for_global.borrow_mut().push(Source {
                        node_name: node_name.to_string(),
                        description,
                        source_type: SourceType::Monitor,
                    });
                }
                _ => {}
            }
        })
        .register();

    while !done.get() {
        mainloop.run();
    }

    let sources = sources.borrow();
    let monitors: Vec<&Source> = sources
        .iter()
        .filter(|s| s.source_type == SourceType::Monitor)
        .collect();
    let inputs: Vec<&Source> = sources
        .iter()
        .filter(|s| s.source_type == SourceType::Input)
        .collect();

    println!("─── Outputs (visualize playback) ─── [{} found]", monitors.len());
    for s in &monitors {
        println!("  🔊 {}", s.description);
        println!("       node.name = {}", s.node_name);
    }
    println!();
    println!("─── Inputs (visualize a microphone or line-in) ─── [{} found]", inputs.len());
    for s in &inputs {
        println!("  🎤 {}", s.description);
        println!("       node.name = {}", s.node_name);
    }

    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!(
        "list-pw-sources is a linux-only diagnostic for the native PipeWire \
         audio backend; on this platform bespec uses cpal and there's nothing \
         to enumerate via PipeWire."
    );
    std::process::exit(2);
}
