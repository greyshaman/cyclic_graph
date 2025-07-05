use std::{
    any::Any,
    collections::{BTreeMap, HashMap, hash_map::Entry},
    sync::{
        Arc, Weak,
        atomic::{AtomicU8, AtomicUsize, Ordering},
    },
};

use super::*;

use async_trait::async_trait;
use cyclic_graph::{Content, Error as CGError, content::layer_content::LayerContent};
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

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

    /// signal emitter id.
    uplink_id: String,

    /// Downlink for receiving from axon of other neuron.
    synaptic_connector: RwLock<broadcast::Receiver<u8>>,
}

impl Synapse {
    /// Create a new Synapse with specified neuron and synaptic connector.
    fn new(
        // Weak reference to neuron owned by this synapse.
        neuron: Weak<Neuron>,

        // identifier of synapse.
        id: usize,

        // Weight of synaptic connector.
        weight: u8,

        // signal emitter id.
        uplink_id: &str,

        // Downlink for receiving from axon of other neuron.
        synaptic_connector: broadcast::Receiver<u8>,
    ) -> Arc<Synapse> {
        let synapse = Arc::new(Self {
            id,
            neuron: neuron.clone(),
            weight,
            cancellation_token: CancellationToken::new(),
            uplink_id: String::from(uplink_id),
            synaptic_connector: RwLock::new(synaptic_connector),
        });

        let c_synapse = synapse.clone();
        tokio::spawn(async move {
            let mut synaptic_connector = c_synapse.synaptic_connector.write().await;
            loop {
                tokio::select! {
                    Ok(signal) = synaptic_connector.recv() => {
                        let signal = signal * c_synapse.weight;
                        match c_synapse.neuron.upgrade() {
                            Some(neuron) => {
                                neuron.operate(signal).await;
                            },
                            None => {
                                break;
                            }
                        }
                    }
                    _ = c_synapse.cancellation_token.cancelled() => {
                        break;
                    }
                }
            }
        });

        synapse
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
    synapses: RwLock<HashMap<String, Arc<Synapse>>>, // Key is downlink id/
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
        let _ = &self.axon.send(signal).expect("Error happened");
    }

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
    /// List of neurons contained in the layer.
    neurons: Arc<RwLock<NeuronsMap>>,
}

impl HiddenLayer {
    /// Create a new HiddenLayer with specified id, network and neurons count.
    pub fn new(id_idx: usize, net_id: &str, neurons_count: usize, thresholds: &[u8]) -> Arc<Self> {
        let layer_id = Arc::new(format!("{}_H{}", net_id.to_string(), id_idx,));
        Arc::new_cyclic(|weak_self| Self {
            me: weak_self.clone(),
            id: layer_id.clone(),
            neurons: Arc::new(RwLock::new((0..neurons_count).fold(
                BTreeMap::new(),
                |mut map, id| {
                    let new_id = format!("{}_Z{}", layer_id, id);
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
        if let Some(src_layer) = link_source_content.as_layer() {
            let src_ids = src_layer.provide_src_ids().await;
            let r_neurons = self.neurons.read().await;
            for dst_neuron in r_neurons.values() {
                for src_id in src_ids.iter() {
                    let mut synapses_binding = dst_neuron.synapses.write().await;
                    if let Entry::Vacant(dendrite) = synapses_binding.entry(src_id.clone()) {
                        let synaptic_connector = src_layer.provide_receiver(src_id.clone()).await?;
                        let weight = 1;
                        let synapse = Synapse::new(
                            dst_neuron.me.clone(),
                            dst_neuron.synapse_id_max.fetch_add(1, Ordering::Acquire),
                            weight,
                            &src_id,
                            synaptic_connector,
                        );
                        dendrite.insert(synapse.clone());

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
        if let Some(src_layer) = link_source_content.as_layer() {
            let src_ids = src_layer.try_provide_src_ids()?;
            let r_neurons = self.neurons.try_read()?;
            for dst_neuron in r_neurons.values() {
                for src_id in src_ids.iter() {
                    let mut synapses_binding = dst_neuron.synapses.try_write()?;
                    if let Entry::Vacant(dendrite) = synapses_binding.entry(src_id.clone()) {
                        let synaptic_connector = src_layer.try_provide_receiver(src_id.clone())?;
                        let weight = 1;
                        let synapse = Synapse::new(
                            dst_neuron.me.clone(),
                            dst_neuron.synapse_id_max.fetch_add(1, Ordering::Acquire),
                            weight,
                            &src_id,
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
                    if let Entry::Occupied(dendrite) = synapses_binding.entry(src_id.clone()) {
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
        if let Some(srrc_layer) = link_source_content.as_layer() {
            let src_ids = srrc_layer.try_provide_src_ids()?;
            let r_neurons = self.neurons.try_read()?;
            for dst_neuron in r_neurons.values() {
                for src_id in src_ids.iter() {
                    let mut synapses_binding = dst_neuron.synapses.try_write()?;
                    if let Entry::Occupied(dendrite) = synapses_binding.entry(src_id.clone()) {
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
