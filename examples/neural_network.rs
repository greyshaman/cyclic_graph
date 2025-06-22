use std::error::Error;

mod layouts {
    pub use std::{
        collections::{HashMap, hash_map::Entry},
        sync::{
            Arc, Weak,
            atomic::{AtomicU8, Ordering},
        },
    };

    use tokio::sync::{RwLock, broadcast};
    use tokio_util::sync::CancellationToken;

    // use crate::network::Network;

    /// The signal channel capacity.
    const CHANNEL_CAPACITY: usize = 5;

    /// The signal sender interface.
    trait SignalSender {
        fn downlink_request(&mut self) -> broadcast::Receiver<u8> {
            self.sender().subscribe()
        }

        fn sender(&self) -> &broadcast::Sender<u8>;
    }

    pub mod input {
        use std::{
            any::Any,
            collections::{BTreeMap, btree_map::Entry},
            sync::{Arc, Weak},
        };

        use async_trait::async_trait;
        use cyclic_graph::{Content, Error as CGError, content::layer_content::LayerContent};
        use tokio::sync::broadcast;

        use crate::network::Network;

        use super::*;

        pub type InputsMap = BTreeMap<String, InputPort>;

        /// The input port is a receiver of the signal and transmit it to connected neurons.
        #[derive(Debug)]
        pub struct InputPort {
            id: Arc<String>,
            // layer: Weak<InputLayer>, // FIXME is it need?
            sender: broadcast::Sender<u8>,
        }

        impl InputPort {
            // pub fn new(id: usize, layer: Weak<InputLayer>) -> Self {
            /// Create a new input port.
            pub fn new(id: Arc<String>) -> Self {
                Self {
                    id: id.clone(),
                    // layer: layer.clone(),
                    sender: broadcast::channel(CHANNEL_CAPACITY).0,
                }
            }

            /// Returns the input port id.
            pub fn id(&self) -> Arc<String> {
                self.id.clone()
            }

            /// Inject the signal to the input port.
            pub fn inject_signal(&self, signal: u8) -> usize {
                let result = self
                    .sender
                    .send(signal)
                    .unwrap_or_else(|err| panic!("Signal sending error: {:?}", err));
                result
            }

            /// Returns true if there are any connected neurons.
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
            me: Weak<InputLayer>,
            network: Weak<Network>,
            inputs: Arc<RwLock<InputsMap>>,
        }

        impl InputLayer {
            /// Create a new input layer with the specified number of inputs.
            pub fn new(network: Weak<Network>, ports_count: usize) -> Arc<Self> {
                Arc::new_cyclic(|weak_self| Self {
                    me: weak_self.clone(),
                    network: network.clone(),
                    inputs: Arc::new(RwLock::new((0..ports_count).fold(
                        BTreeMap::new(),
                        |mut map, id| {
                            let net = network.upgrade().expect("specified network not found");
                            let new_id = format!("{}_I_{}", net.id(), id);
                            map.insert(new_id.clone(), InputPort::new(Arc::new(new_id)));
                            map
                        },
                    ))),
                })
            }

            /// Sends a signal to the specified port.
            pub async fn send_to(&self, signal: u8, to_port: &'static str) -> usize {
                let input_binding = self.inputs.read().await;
                let r_input_port = input_binding
                    .get(&to_port.to_string())
                    .expect("Incorrect input port id");
                r_input_port.inject_signal(signal)
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
    }

    pub mod hidden {
        use std::{any::Any, collections::BTreeMap, sync::atomic::AtomicUsize};

        use crate::network::Network;

        use super::*;

        use async_trait::async_trait;
        use cyclic_graph::{Content, Error as CGError, content::layer_content::LayerContent};

        /// The NeuronsMap is collection of neurons.
        pub type NeuronsMap = BTreeMap<String, Arc<Neuron>>;

        /// The Synapse is connector between other neuron's axon
        /// and self neuron's dendrite.
        #[derive(Debug)]
        struct Synapse {
            /// Identifier of synaptic connector.
            id: usize,

            /// Weak reference to neuron owned by this synapse.
            neuron: Weak<Neuron>,

            /// Weight of synaptic connector.
            weight: u8,

            /// Token for cancellation of receiving from synaptic connector.
            /// Used to stop receiving from synaptic connector.
            cancellation_token: CancellationToken,

            /// Downlink for receiving from axon of other neuron.
            synaptic_connector: broadcast::Receiver<u8>,
        }

        impl Synapse {
            /// Create a new Synapse with specified neuron and synaptic connector.
            fn new(
                neuron: Weak<Neuron>,
                id: usize,
                weight: u8,
                synaptic_connector: broadcast::Receiver<u8>,
            ) -> Self {
                Self {
                    id,
                    neuron: neuron.clone(),
                    weight,
                    cancellation_token: CancellationToken::new(),
                    synaptic_connector,
                }
            }
        }

        impl Drop for Synapse {
            /// Drop the Synapse and stop receiving from synaptic connector.
            fn drop(&mut self) {
                self.cancellation_token.cancel();
            }
        }

        /// The Neuron is Hidden Layer content element
        #[derive(Debug)]
        pub struct Neuron {
            /// Identifier of neuron.
            id: Arc<String>,
            /// Weak reference to neuron owned by this synapse.
            me: Weak<Neuron>,
            // layer: Weak<HiddenLayer>,
            /// Atomic counter of neuron's activation
            accumulator: AtomicU8,
            /// Reset neuron's activation counter.
            threshold: RwLock<u8>,
            /// Incoming synaptic connectors.
            synapses: RwLock<HashMap<String, Synapse>>, // Key is downlink id/
            /// Atomic synapse id generator.
            synapse_id_max: AtomicUsize,
            /// Outgoing axon connector.
            axon: broadcast::Sender<u8>,
        }

        impl Neuron {
            /// Create a new Neuron with specified id and layer.
            pub fn new(id: Arc<String>, _layer: Weak<HiddenLayer>, threshold: u8) -> Arc<Self> {
                Arc::new_cyclic(|weak_self| Neuron {
                    id: id.clone(),
                    me: weak_self.clone(),
                    // layer: layer.clone(),
                    accumulator: AtomicU8::new(0),
                    threshold: RwLock::new(threshold),
                    synapses: RwLock::new(HashMap::new()),
                    synapse_id_max: AtomicUsize::new(0),
                    axon: broadcast::channel(CHANNEL_CAPACITY).0,
                })
            }

            /// Perform neuron's operation with weighted signal.
            async fn operate(&self, weighted_signal: u8) {
                loop {
                    let current_value = self.accumulator.load(Ordering::Acquire);
                    let new_value = current_value + weighted_signal;

                    if new_value >= *self.threshold.read().await {
                        if self
                            .accumulator
                            .compare_exchange(
                                current_value,
                                new_value,
                                Ordering::Release,
                                Ordering::Acquire,
                            )
                            .is_ok()
                        {
                            self.accumulator.store(0, Ordering::Release);
                            self.send(new_value).await;
                            return;
                        }
                    }

                    if self
                        .accumulator
                        .compare_exchange(
                            current_value,
                            new_value,
                            Ordering::Release,
                            Ordering::Acquire,
                        )
                        .is_ok()
                    {
                        return;
                    }
                }
            }

            /// Send signal to connected downstream neurons.
            async fn send(&self, signal: u8) {
                &self.axon.send(signal).expect("Error happened");
            }

            // fn get_layer(&self) -> Option<Arc<HiddenLayer>> {
            //     self.layer.upgrade()
            // }

            /// Return Neuron's identifier.
            fn id(&self) -> Arc<String> {
                self.id.clone()
            }
        }

        impl SignalSender for Neuron {
            fn sender(&self) -> &broadcast::Sender<u8> {
                &self.axon
            }
        }

        /// The HiddenLayer is a collection of neurons.
        #[derive(Debug)]
        pub struct HiddenLayer {
            /// Self-weak reference.
            me: Weak<HiddenLayer>,
            /// Unique identifier of the layer.
            id: Arc<String>,
            /// Reference to the network containing the layer.
            network: Weak<Network>,
            /// List of neurons contained in the layer.
            neurons: Arc<RwLock<NeuronsMap>>,
        }

        impl HiddenLayer {
            /// Create a new HiddenLayer with specified id, network and neurons count.
            pub fn new(
                id_idx: usize,
                network: Weak<Network>,
                neurons_count: usize,
                thresholds: &[u8],
            ) -> Arc<Self> {
                let layer_id = Arc::new(format!(
                    "{}_H{}",
                    network
                        .upgrade()
                        .map(|net| net.id())
                        .unwrap_or("N9999".to_string()),
                    id_idx,
                ));
                Arc::new_cyclic(|weak_self| Self {
                    me: weak_self.clone(),
                    id: layer_id.clone(),
                    network: network.clone(),
                    neurons: Arc::new(RwLock::new((0..neurons_count).fold(
                        BTreeMap::new(),
                        |mut map, id| {
                            let new_id = format!("{}_{}", layer_id, id);
                            map.insert(
                                new_id.clone(),
                                Neuron::new(
                                    // new_id,
                                    Arc::new(new_id),
                                    weak_self.clone(),
                                    *thresholds.get(id).unwrap_or(&1),
                                ),
                            );
                            map
                        },
                    ))),
                })
            }
        }

        #[async_trait]
        impl LayerContent for HiddenLayer {
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
                let r_neurons = self.neurons.read().await;
                match r_neurons.get(&src_idx) {
                    Some(neuron) => Ok(neuron.axon.subscribe()),
                    None => Err(CGError::LinksProviderHandlerError(
                        "neuron not found".into(),
                    )),
                }
            }

            fn try_provide_receiver(
                &self,
                src_idx: String,
            ) -> Result<broadcast::Receiver<u8>, CGError<String>> {
                let r_neurons = self.neurons.try_read().expect("TryLock error");
                match r_neurons.get(&src_idx) {
                    Some(neuron) => Ok(neuron.axon.subscribe()),
                    None => Err(CGError::LinksProviderHandlerError(
                        "neuron not found".into(),
                    )),
                }
            }

            async fn provide_src_ids(&self) -> Vec<String> {
                self.neurons
                    .read()
                    .await
                    .keys()
                    .cloned()
                    .collect::<Vec<String>>()
            }

            fn try_provide_src_ids(&self) -> Result<Vec<String>, CGError<String>> {
                let ids = self
                    .neurons
                    .try_read()?
                    .keys()
                    .cloned()
                    .collect::<Vec<String>>();
                Ok(ids)
            }

            async fn connect(
                &self,
                link_source_content: &Content<String, (), u8>,
            ) -> Result<bool, CGError<String>> {
                let mut result = true;
                if let Some(layer) = link_source_content.as_layer() {
                    let src_ids = layer.provide_src_ids().await;
                    let r_neurons = self.neurons.read().await;
                    for dst_neuron in r_neurons.values() {
                        for src_id in src_ids.iter() {
                            let mut synapses_binding = dst_neuron.synapses.write().await;
                            if let Entry::Vacant(dendrite) = synapses_binding.entry(src_id.clone())
                            {
                                let synaptic_connector =
                                    layer.provide_receiver(src_id.clone()).await?;
                                let weight = 1;
                                let synapse = Synapse::new(
                                    dst_neuron.me.clone(),
                                    dst_neuron.synapse_id_max.fetch_add(1, Ordering::Acquire),
                                    weight,
                                    synaptic_connector,
                                );
                                dendrite.insert(synapse);
                                result &= true;
                            } else {
                                // synapse already exists
                                result &= false;
                            }
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
                if let Some(layer) = link_source_content.as_layer() {
                    let src_ids = layer.try_provide_src_ids()?;
                    let r_neurons = self.neurons.try_read()?;
                    for dst_neuron in r_neurons.values() {
                        for src_id in src_ids.iter() {
                            let mut synapses_binding = dst_neuron.synapses.try_write()?;
                            if let Entry::Vacant(dendrite) = synapses_binding.entry(src_id.clone())
                            {
                                let synaptic_connector =
                                    layer.try_provide_receiver(src_id.clone())?;
                                let weight = 1;
                                let synapse = Synapse::new(
                                    dst_neuron.me.clone(),
                                    dst_neuron.synapse_id_max.fetch_add(1, Ordering::Acquire),
                                    weight,
                                    synaptic_connector,
                                );
                                dendrite.insert(synapse);
                                result &= true;
                            } else {
                                // synapse already exists
                                result &= false;
                            }
                        }
                    }
                }
                Ok(result)
            }

            async fn disconnect(
                &self,
                link_source_content: &Content<String, (), u8>,
            ) -> Result<bool, CGError<String>> {
                let mut result = true;
                if let Some(layer) = link_source_content.as_layer() {
                    let src_ids = layer.provide_src_ids().await;
                    let r_neurons = self.neurons.read().await;
                    for dst_neuron in r_neurons.values() {
                        for src_id in src_ids.iter() {
                            let mut synapses_binding = dst_neuron.synapses.write().await;
                            if let Entry::Occupied(dendrite) =
                                synapses_binding.entry(src_id.clone())
                            {
                                dendrite.remove();
                                result &= true;
                            } else {
                                // synapse does not exist
                                result &= false;
                            }
                        }
                    }
                }
                Ok(result)
            }

            fn try_disconnect(
                &self,
                link_source_content: &Content<String, (), u8>,
            ) -> Result<bool, CGError<String>> {
                let mut result = true;
                if let Some(layer) = link_source_content.as_layer() {
                    let src_ids = layer.try_provide_src_ids()?;
                    let r_neurons = self.neurons.try_read()?;
                    for dst_neuron in r_neurons.values() {
                        for src_id in src_ids.iter() {
                            let mut synapses_binding = dst_neuron.synapses.try_write()?;
                            if let Entry::Occupied(dendrite) =
                                synapses_binding.entry(src_id.clone())
                            {
                                dendrite.remove();
                                result &= true;
                            } else {
                                // synapse does not exist
                                result &= false;
                            }
                        }
                    }
                }

                Ok(result)
            }
        }
    }

    pub mod output {
        use std::{any::Any, collections::BTreeMap};

        use async_trait::async_trait;
        use cyclic_graph::{Content, Error as CGError, content::layer_content::LayerContent};

        use crate::network::Network;

        use super::*;

        /// Output ports collection.
        pub type OutputsMap = BTreeMap<String, OutputPort>;

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

            // fn connect(&mut self, rx: broadcast::Receiver<u8>) {
            //     if self.receiver.is_none() {
            //         self.receiver = Some(rx);
            //     } else {
            //         panic!("Port {} already connected", self.id);
            //     }
            // }

            /// Returns `id` of this port.
            fn id(&self) -> Arc<String> {
                self.id.clone()
            }

            /// Returns `true` if this port is connected, `false` otherwise.
            fn is_connected(&self) -> bool {
                self.receiver.is_some()
            }

            // fn disconnect(&mut self) {
            //     if self.receiver.is_some() {
            //         self.receiver = None;
            //     }
            // }
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
    }
}

mod network {
    use std::{
        error::Error,
        sync::{Arc, Weak},
    };

    use cyclic_graph::{Content, CyclicGraph};
    use tokio::sync::RwLock;

    use crate::layouts::{input::InputLayer, output::OutputLayer};

    pub struct Network {
        me: Weak<Network>,
        id: String,
        content: CyclicGraph<String, (), u8>,
    }

    impl Network {
        pub fn new(
            id: usize,
            input_ports_number: usize,
            output_ports_number: usize,
        ) -> Result<Arc<Self>, Box<dyn Error>> {
            let net = Arc::new_cyclic(|weak_network| {
                let input_layer = InputLayer::new(weak_network.clone(), input_ports_number);

                let output_layer = OutputLayer::new(weak_network.clone(), output_ports_number);
                Self {
                    id: format!("N_{id}"),
                    me: weak_network.clone(),
                    content: CyclicGraph::new_default(
                        String::from("IL"),
                        Content::new_layer(input_layer),
                        String::from("OL"),
                        Content::new_layer(output_layer),
                        0_usize,
                    )
                    .expect("Cannot create cyclic graph instance"),
                }
            });
            Ok(net)
        }

        pub fn id(&self) -> String {
            self.id.clone()
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    Ok(())
}
