use std::{
    any::Any,
    collections::{BTreeMap, btree_map::Entry},
    sync::Arc,
};

use async_trait::async_trait;
use cyclic_graph::{Content, Error as CGError, LayerContent};
use tokio::sync::{RwLock, broadcast};

use crate::layouts::{CHANNEL_CAPACITY, SignalSender};

/// Input ports collection
pub type InputsMap = BTreeMap<String, InputPort>;

/// The input port is a receiver of the signal and transmit it to connected neurons.
#[derive(Debug)]
pub struct InputPort {
    id: Arc<String>,
    sender: broadcast::Sender<u8>,
}

impl InputPort {
    // pub fn new(id: usize, layer: Weak<InputLayer>) -> Self {
    /// Create a new input port.
    pub fn new(id: Arc<String>) -> Self {
        Self {
            id: id.clone(),
            sender: broadcast::channel(CHANNEL_CAPACITY).0,
        }
    }

    /// Returns the input port id.
    #[allow(dead_code)]
    pub fn id(&self) -> Arc<String> {
        self.id.clone()
    }

    /// Inject the signal to the input port.
    /// Returns number of sent signals or Sender error.
    pub fn inject_signal(&self, signal: u8) -> Result<usize, broadcast::error::SendError<u8>> {
        self.sender.send(signal)
    }

    /// Returns true if there are any connected neurons.
    #[allow(dead_code)]
    pub fn is_connected(&self) -> bool {
        self.sender.receiver_count() > 0
    }
}

impl SignalSender for InputPort {
    fn sender(&self) -> &broadcast::Sender<u8> {
        &self.sender
    }
}

/// The input layer is a container for input ports which allows
/// to inject signals from outside.
#[derive(Debug)]
pub struct InputLayer {
    inputs: Arc<RwLock<InputsMap>>,
}

impl InputLayer {
    /// Create a new input layer with the specified number of inputs.
    pub fn new(net_id: &str, ports_count: usize) -> Arc<Self> {
        Arc::new(Self {
            inputs: Arc::new(RwLock::new((0..ports_count).fold(
                BTreeMap::new(),
                |mut map, id| {
                    let new_id = format!("{}_IL{}", net_id, id);
                    map.insert(new_id.clone(), InputPort::new(Arc::new(new_id)));
                    map
                },
            ))),
        })
    }

    /// Sends a signal to the specified port.
    pub async fn send_to(
        &self,
        signal: u8,
        to_port: String,
    ) -> Result<usize, broadcast::error::SendError<u8>> {
        let input_binding = self.inputs.read().await;
        let r_input_port = input_binding
            .get(&to_port)
            .expect("Incorrect input port id");
        r_input_port.inject_signal(signal)
    }

    /// Returns vector with port ids
    pub async fn port_ids(&self) -> Vec<String> {
        let r_inputs = self.inputs.read().await;
        r_inputs.keys().cloned().collect()
    }
}

#[async_trait]
impl LayerContent for InputLayer {
    type IdType = String;
    type PayloadType = ();
    type SignalType = u8;

    fn as_any(&self) -> &dyn Any {
        self
    }

    async fn provide_receiver(
        &self,
        src_idx: String,
    ) -> Result<broadcast::Receiver<u8>, CGError<String>> {
        let mut r_inputs = self.inputs.write().await;
        match r_inputs.entry(src_idx) {
            Entry::Occupied(entry) => Ok(entry.get().sender.subscribe()),
            Entry::Vacant(_) => Err(CGError::LinksProviderHandlerError(
                "Incorrect input port id".into(),
            )),
        }
    }

    fn try_provide_receiver(
        &self,
        src_idx: String,
    ) -> Result<broadcast::Receiver<u8>, CGError<String>> {
        let mut r_inputs = self.inputs.try_write().expect("Try lock error");
        match r_inputs.entry(src_idx) {
            Entry::Occupied(entry) => Ok(entry.get().sender.subscribe()),
            Entry::Vacant(_) => Err(CGError::LinksProviderHandlerError(
                "Incorrect input port id".into(),
            )),
        }
    }

    async fn provide_src_ids(&self) -> Vec<String> {
        let r_inputs = self.inputs.read().await;
        r_inputs.keys().cloned().collect::<Vec<String>>()
    }

    fn try_provide_src_ids(&self) -> Result<Vec<String>, CGError<String>> {
        let ids = self
            .inputs
            .try_read()?
            .keys()
            .cloned()
            .collect::<Vec<String>>();
        Ok(ids)
    }

    async fn connect(
        &self,
        _link_source_content: &Content<String, (), u8>,
    ) -> Result<bool, CGError<String>> {
        Err(CGError::LinksAcceptorHandlerError(
            "Connect does not implemented for input layer".into(),
        ))
    }

    fn try_connect(
        &self,
        _link_source_content: &Content<String, (), u8>,
    ) -> Result<bool, CGError<String>> {
        Err(CGError::LinksAcceptorHandlerError(
            "Connect does not implemented for input layer".into(),
        ))
    }

    async fn disconnect(
        &self,
        _link_source_content: &Content<String, (), u8>,
    ) -> Result<bool, CGError<String>> {
        Err(CGError::LinksAcceptorHandlerError(
            "Disconnect does not implemented for input layer".into(),
        ))
    }

    fn try_disconnect(
        &self,
        _link_source_content: &Content<String, (), u8>,
    ) -> Result<bool, CGError<String>> {
        Err(CGError::LinksAcceptorHandlerError(
            "Disconnect does not implemented for input layer".into(),
        ))
    }
}
