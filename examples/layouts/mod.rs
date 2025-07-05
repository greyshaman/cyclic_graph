use tokio::sync::broadcast;

pub const CHANNEL_CAPACITY: usize = 20;

pub(crate) mod hidden;
pub(crate) mod input;
pub(crate) mod output;

/// The signal sender interface.
trait SignalSender {
    fn downlink_request(&mut self) -> broadcast::Receiver<u8> {
        self.sender().subscribe()
    }

    fn sender(&self) -> &broadcast::Sender<u8>;
}
