use std::{any::Any, collections::BTreeMap, sync::Arc};

use async_trait::async_trait;
use cyclic_graph::{Content, Error as CGError, content::layer_content::LayerContent};
use futures::stream::select_all;
use tokio::sync::RwLock;
use tokio_stream::{StreamExt, wrappers::BroadcastStream};

use super::*;

/// Output ports collection.
/// The key contains id of output port, value contains channel receiver data (source Id, receiver)
pub type OutputsMap = BTreeMap<String, OutputPort>;

/// Results data format provided by output stream.
/// The first one is id of output port, second one is data.
pub struct OutputData(pub String, pub u8);

/// This is the output of the network.
#[derive(Debug)]
pub struct OutputPort {
    id: Arc<String>,
    uplink_id: Option<String>,
    receiver: Option<broadcast::Receiver<u8>>,
}

impl OutputPort {
    /// Creates a new instance of `OutputPort`.
    pub fn new(id: &str) -> Self {
        Self {
            id: Arc::new(id.to_string()),
            uplink_id: None,
            receiver: None,
        }
    }

    /// Returns `id` of this port.
    pub fn id(&self) -> Arc<String> {
        self.id.clone()
    }

    /// Returns `true` if this port is connected, `false` otherwise.
    pub fn is_connected(&self) -> bool {
        self.receiver.is_some()
    }
}

#[derive(Debug)]
pub struct OutputLayer {
    outputs: Arc<RwLock<OutputsMap>>,
}

impl OutputLayer {
    /// Creates a new instance of `OutputLayer`.
    pub fn new(net_id: &str, ports_count: usize) -> Arc<Self> {
        Arc::new(Self {
            outputs: Arc::new(RwLock::new((0..ports_count).fold(
                BTreeMap::new(),
                |mut map, id| {
                    let new_id = format!("{}_OL{}", net_id.to_string(), id);
                    map.insert(new_id.clone(), OutputPort::new(&new_id));
                    map
                },
            ))),
        })
    }

    /// Returns output_stream
    pub async fn into_stream(&self) -> impl tokio_stream::Stream<Item = OutputData> + '_ {
        let r_outputs = self.outputs.read().await;

        let streams = r_outputs.values().filter_map(|port| {
            port.receiver.as_ref().map(|rx| {
                let id = port.id();
                BroadcastStream::new(rx.resubscribe()).filter_map(move |res| {
                    let id = id.to_string();
                    match res {
                        Ok(value) => Some(OutputData(id, value)),
                        Err(e) => {
                            eprintln!("Error in port {}: {:?}", id, e);
                            None
                        }
                    }
                })
            })
        });
        select_all(streams)
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

    /// Connect output ports with uplink signal emitters one by one.
    async fn connect(
        &self,
        link_source_content: &Content<String, (), u8>,
    ) -> Result<bool, CGError<String>> {
        let mut result = true;

        // Gets source layer content
        if let Some(src_layer) = link_source_content.as_layer() {
            // Collects source signal emitter ids
            let src_ids = src_layer.provide_src_ids().await;

            // Gets available output port ids
            let output_ids = {
                let r_outputs = self.outputs.read().await;
                r_outputs.keys().cloned().collect::<Vec<_>>()
            };

            let mut w_outputs = self.outputs.write().await;

            // Iterate source ids with index from ids list
            for (idx, src_id) in src_ids.into_iter().enumerate() {
                // check if output port available for specified index and get mutable output port in success.
                if idx < output_ids.len()
                    && let Some(dst_port) = w_outputs.get_mut(&output_ids[idx])
                {
                    // subscribe receivers to uplink channel
                    let rx = src_layer.provide_receiver(src_id.clone()).await?;

                    // store uplink_id and receiver
                    dst_port.uplink_id = Some(src_id);
                    dst_port.receiver = Some(rx);
                    result &= true
                }
            }
        }

        Ok(result)
    }

    fn try_connect(
        &self,
        link_source_content: &Content<String, (), u8>,
    ) -> Result<bool, CGError<String>> {
        let mut result = true;

        // Gets source layer content
        if let Some(src_layer) = link_source_content.as_layer() {
            // Collects source signal emitter ids
            let src_ids = src_layer.try_provide_src_ids()?;

            //Gets available output port ids
            let output_ids = {
                let r_ouputs = self.outputs.try_read()?;
                r_ouputs.keys().cloned().collect::<Vec<_>>()
            };

            let mut w_outputs = self.outputs.try_write()?;

            // Iterate source ids with index from ids list
            for (idx, src_id) in src_ids.into_iter().enumerate() {
                if idx < output_ids.len()
                    && let Some(dst_port) = w_outputs.get_mut(&output_ids[idx])
                {
                    let rx = src_layer.try_provide_receiver(src_id.clone())?;

                    dst_port.uplink_id = Some(src_id);
                    dst_port.receiver = Some(rx);
                    result &= true;
                }
            }
        }

        Ok(result)
    }

    // The output ports have single connection independently of nature of uplink content items
    // then remove receiver data and uplink_id.
    async fn disconnect(
        &self,
        _link_source_content: &Content<String, (), u8>,
    ) -> Result<bool, CGError<String>> {
        let mut result = true;
        let mut w_outputs = self.outputs.write().await;
        for output in w_outputs.values_mut() {
            output.uplink_id = None;
            output.receiver = None;
            result &= true;
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
            output.uplink_id = None;
            output.receiver = None;
            result &= true;
        }

        Ok(result)
    }
}
