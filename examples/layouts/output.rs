use std::{
    any::Any,
    collections::BTreeMap,
    sync::{Arc, Weak},
};

use async_trait::async_trait;
use cyclic_graph::{Content, Error as CGError, content::layer_content::LayerContent};
use tokio::sync::RwLock;

use crate::network::Network;

use super::*;

/// Output ports collection.
type OutputsMap = BTreeMap<String, OutputPort>;

/// This is the output of the network.
#[derive(Debug)]
struct OutputPort {
    id: Arc<String>,
    receiver: Option<broadcast::Receiver<u8>>,
}

impl OutputPort {
    /// Creates a new instance of `OutputPort`.
    fn new(id: Arc<String>) -> Self {
        Self {
            id: Arc::new(format!("O_{}", id)),
            receiver: None,
        }
    }

    /// Returns `id` of this port.
    fn id(&self) -> Arc<String> {
        self.id.clone()
    }

    /// Returns `true` if this port is connected, `false` otherwise.
    fn is_connected(&self) -> bool {
        self.receiver.is_some()
    }
}

#[derive(Debug)]
pub struct OutputLayer {
    me: Weak<OutputLayer>,
    network: Weak<Network>,
    outputs: Arc<RwLock<OutputsMap>>,
}

impl OutputLayer {
    /// Creates a new instance of `OutputLayer`.
    pub fn new(network: Weak<Network>, ports_count: usize) -> Arc<Self> {
        Arc::new_cyclic(|weak_self| Self {
            me: weak_self.clone(),
            network: network.clone(),
            outputs: Arc::new(RwLock::new((0..ports_count).fold(
                BTreeMap::new(),
                |mut map, id| {
                    let net = network.upgrade().expect("Network not found");
                    let new_id = format!("{}_O_{}", net.id(), id);
                    map.insert(new_id.clone(), OutputPort::new(Arc::new(new_id)));
                    map
                },
            ))),
        })
    }
}

#[async_trait]
impl LayerContent for OutputLayer {
    type IdType = String;
    type PayloadType = ();
    type SignalType = u8;

    fn as_any(&self) -> &dyn Any {
        self
    }

    async fn provide_receiver(
        &self,
        _src_idx: String,
    ) -> Result<broadcast::Receiver<u8>, CGError<String>> {
        Err(CGError::NotImplemented(
            "provide_receiver does not implemented for OutputLayer".into(),
        ))
    }

    fn try_provide_receiver(
        &self,
        _src_idx: String,
    ) -> Result<broadcast::Receiver<u8>, CGError<String>> {
        Err(CGError::NotImplemented(
            "try_provide_receiver does not implemented for OutputLayer".into(),
        ))
    }

    async fn provide_src_ids(&self) -> Vec<String> {
        vec![]
    }

    fn try_provide_src_ids(&self) -> Result<Vec<String>, CGError<String>> {
        Ok(vec![])
    }

    async fn connect(
        &self,
        link_source_content: &Content<String, (), u8>,
    ) -> Result<bool, CGError<String>> {
        let mut result = false;
        if let Some(layer) = link_source_content.as_layer() {
            let src_ids = layer.provide_src_ids().await;
            let mut w_outputs = self.outputs.write().await;
            for src_id in src_ids.iter() {
                let rx = layer.provide_receiver(src_id.clone()).await?;
                if let Some(port) = w_outputs.get_mut(src_id) {
                    port.receiver = Some(rx);
                    result &= true;
                } else {
                    // port not found
                    result &= false;
                }
            }
        }

        Ok(result)
    }

    fn try_connect(
        &self,
        link_source_content: &Content<String, (), u8>,
    ) -> Result<bool, CGError<String>> {
        let mut result = false;
        if let Some(layer) = link_source_content.as_layer() {
            let src_ids = layer.try_provide_src_ids()?;
            let mut w_outputs = self.outputs.try_write()?;
            for src_id in src_ids.iter() {
                let rx = layer.try_provide_receiver(src_id.clone())?;
                if let Some(port) = w_outputs.get_mut(src_id) {
                    port.receiver = Some(rx);
                    result &= true;
                } else {
                    // port not found
                    result &= false;
                }
            }
        }

        Ok(result)
    }

    async fn disconnect(
        &self,
        _link_source_content: &Content<String, (), u8>,
    ) -> Result<bool, CGError<String>> {
        let mut result = true;
        let mut w_outputs = self.outputs.write().await;
        for output in w_outputs.values_mut() {
            if output.is_connected() {
                output.receiver = None;
                result &= true;
            } else {
                result &= false;
            }
        }

        Ok(result)
    }

    fn try_disconnect(
        &self,
        _link_source_content: &Content<String, (), u8>,
    ) -> Result<bool, CGError<String>> {
        let mut result = true;
        let mut w_outputs = self.outputs.try_write()?;
        for output in w_outputs.values_mut() {
            if output.is_connected() {
                output.receiver = None;
                result &= true;
            } else {
                result &= false;
            }
        }

        Ok(result)
    }
}
