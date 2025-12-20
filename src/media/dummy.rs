use crossbeam_channel::Sender;
use super::{MediaController, MediaMonitor, MediaTrackInfo};

pub struct DummyMediaManager;
impl DummyMediaManager { pub fn new() -> Self { Self } }

impl MediaController for DummyMediaManager {
    fn try_play_pause(&self) {}
    fn try_next(&self) {}
    fn try_prev(&self) {}
}

impl MediaMonitor for DummyMediaManager {
    fn start(&self, _tx: Sender<MediaTrackInfo>) {
        println!("Media Monitor: Not supported on this OS.");
    }
}