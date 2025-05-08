use std::error::Error;

mod layouts {
    use std::{
        collections::{HashMap, hash_map::Entry},
        sync::{
            Arc, Weak,
            atomic::{AtomicU8, Ordering},
        },
    };

    use tokio::sync::{RwLock, broadcast};
    use tokio_util::sync::CancellationToken;

    use crate::network::Network;

    const CHANNEL_CAPACITY: usize = 5;

    trait SignalSender {
        fn downlink_request(&mut self) -> broadcast::Receiver<u8> {
            if let Some(tx) = self.sender() {
                tx.subscribe()
            } else {
                let (tx, rx) = broadcast::channel(CHANNEL_CAPACITY);
                self.set_sender(tx);
                rx
            }
        }

        fn sender(&self) -> Option<broadcast::Sender<u8>>;

        fn set_sender(&mut self, tx: broadcast::Sender<u8>);
    }

    pub struct InputPort {
        id: Arc<String>,
        layer: Weak<InputLayer>,
        sender: Option<broadcast::Sender<u8>>,
    }

    impl InputPort {
        pub fn new(id: usize, layer: Weak<InputLayer>) -> Self {
            Self {
                id: Arc::new(format!("I_{}", id)),
                layer: layer.clone(),
                sender: None,
            }
        }

        pub fn id(&self) -> Arc<String> {
            self.id.clone()
        }

        pub fn send(&self, signal: u8) -> usize {
            if let Some(sender) = self.sender.as_ref() {
                let result = sender
                    .send(signal)
                    .unwrap_or_else(|err| panic!("Signal sending error: {:?}", err));
                result
            } else {
                panic!("Cannot send without connection");
            }
        }

        pub fn is_connected(&self) -> bool {
            self.sender.is_some()
        }
    }

    impl SignalSender for InputPort {
        fn sender(&self) -> Option<broadcast::Sender<u8>> {
            self.sender.clone()
        }

        fn set_sender(&mut self, tx: broadcast::Sender<u8>) {
            self.sender = Some(tx);
        }
    }

    pub struct InputLayer {
        me: Weak<InputLayer>,
        network: Weak<Network>,
        inputs: Vec<Arc<RwLock<InputPort>>>,
    }

    impl InputLayer {
        pub fn new(network: Weak<Network>, ports_number: usize) -> Arc<Self> {
            Arc::new_cyclic(|weak_self| Self {
                me: weak_self.clone(),
                network: network.clone(),
                inputs: (0..ports_number)
                    .map(|id| Arc::new(RwLock::new(InputPort::new(id, weak_self.clone()))))
                    .collect(),
            })
        }

        pub async fn send_to(&self, signal: u8, to_port: usize) -> usize {
            let input_port = self
                .inputs
                .get(to_port)
                .expect("Incorrect input port number");
            let r_input_port = input_port.read().await;
            r_input_port.send(signal)
        }

        // link to each output port or neuron from each input port
        pub async fn link_to(&self, dst_layer: Layer, weights: &[u8]) {
            match dst_layer {
                Layer::Hidden(layer) => {
                    let number_of_links = std::cmp::min(self.inputs.len(), layer.neurons.len());
                    for index in 0..number_of_links {
                        let mut w_input = self
                            .inputs
                            .get(index)
                            .expect("Cannot access to input port by invalid index")
                            .write()
                            .await;
                        let r_neuron = layer
                            .neurons
                            .get(index)
                            .expect("Cannot access to neuron by invalid index")
                            .read()
                            .await;

                        if !w_input.is_connected() {
                            let rx = w_input.downlink_request();
                            r_neuron
                                .connect(rx, *weights.get(index).unwrap_or(&1), w_input.id())
                                .await;
                        }
                    }
                }
                Layer::Output(layer) => {
                    let number_of_links = std::cmp::min(self.inputs.len(), layer.outputs.len());
                    for index in 0..number_of_links {
                        let mut w_input = self
                            .inputs
                            .get(index)
                            .expect("cannot access to input port by invalid index")
                            .write()
                            .await;
                        let mut w_output = layer
                            .outputs
                            .get(index)
                            .expect("cannot access to output port by invalid index")
                            .write()
                            .await;

                        if !w_input.is_connected() {
                            let rx = w_input.downlink_request();
                            w_output.connect(rx);
                        }
                    }
                }
                Layer::Input(_) => {
                    panic!("Its not possible to establish link between two input layers")
                }
            }
        }
    }

    struct Synapse {
        id: usize,
        neuron: Weak<RwLock<Neuron>>,
        weight: u8,
        cancellation_token: CancellationToken,
    }

    impl Synapse {
        fn new(neuron: Weak<RwLock<Neuron>>, id: usize, weight: u8) -> Self {
            Self {
                id,
                neuron: neuron.clone(),
                weight,
                cancellation_token: CancellationToken::new(),
            }
        }
    }

    impl Drop for Synapse {
        fn drop(&mut self) {
            self.cancellation_token.cancel();
        }
    }

    pub struct Neuron {
        id: Arc<String>,
        me: Weak<RwLock<Neuron>>,
        layer: Weak<HiddenLayer>,
        accumulator: AtomicU8,
        threshold: RwLock<u8>,
        synapses: RwLock<HashMap<Arc<String>, Synapse>>,
        sender: Option<broadcast::Sender<u8>>,
    }

    impl Neuron {
        pub fn new(id: usize, layer: Weak<HiddenLayer>, threshold: u8) -> Arc<RwLock<Self>> {
            Arc::new_cyclic(|weak_self| {
                RwLock::new(Self {
                    id: Arc::new(format!("Z_{}", id)),
                    me: weak_self.clone(),
                    layer: layer.clone(),
                    accumulator: AtomicU8::new(0),
                    threshold: RwLock::new(threshold),
                    synapses: RwLock::new(HashMap::new()),
                    sender: None,
                })
            })
        }

        pub async fn connect(
            &self,
            mut receiver: broadcast::Receiver<u8>,
            weight: u8,
            source_id: Arc<String>,
        ) {
            let mut w_synapses = self.synapses.write().await;
            let new_id = w_synapses
                .values()
                .map(|s| s.id)
                .max()
                .and_then(|id| Some(id + 1))
                .unwrap_or_default();
            if let Entry::Vacant(entry) = w_synapses.entry(source_id.clone()) {
                entry.insert(Synapse::new(self.me.clone(), new_id, weight));
                drop(w_synapses);

                let weak_self = self.me.clone();
                tokio::spawn(async move {
                    if let Some(neuron) = weak_self.upgrade() {
                        let r_neuron = neuron.read().await;
                        let r_synapses = r_neuron.synapses.read().await;
                        if let Some(synapse) = r_synapses.get(source_id.as_ref()) {
                            loop {
                                tokio::select! {
                                    _ = synapse.cancellation_token.cancelled() => {
                                        println!("synapse id: {} disconnected", synapse.id);
                                        break;
                                    }

                                    signal = receiver.recv() => {
                                        match signal {
                                            Ok(value) => {
                                                neuron.read().await.operate(synapse.weight * value).await;
                                            }
                                            Err(_) => break,
                                        }
                                    }
                                }
                            }
                        }
                    }
                });
            }
        }

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

        async fn send(&self, signal: u8) {
            if let Some(tx) = &self.sender {
                tx.send(signal).expect("Error happened");
            }
        }

        // fn get_layer(&self) -> Option<Arc<HiddenLayer>> {
        //     self.layer.upgrade()
        // }

        fn id(&self) -> Arc<String> {
            self.id.clone()
        }
    }

    impl SignalSender for Neuron {
        fn sender(&self) -> Option<broadcast::Sender<u8>> {
            self.sender.clone()
        }

        fn set_sender(&mut self, tx: broadcast::Sender<u8>) {
            self.sender = Some(tx);
        }
    }

    pub struct HiddenLayer {
        me: Weak<HiddenLayer>,
        network: Weak<Network>,
        neurons: Vec<Arc<RwLock<Neuron>>>,
    }

    impl HiddenLayer {
        pub fn new(network: Arc<Network>, neurons_number: usize, threshold: u8) -> Arc<Self> {
            Arc::new_cyclic(|weak_self| Self {
                me: weak_self.clone(),
                network: Arc::downgrade(&network),
                neurons: (0..neurons_number)
                    .map(|id| Neuron::new(id, weak_self.clone(), threshold))
                    .collect(),
            })
        }

        pub async fn link_to(&self, dst_layer: Layer, weights: &[u8]) {
            match dst_layer {
                Layer::Hidden(layer) => {
                    let src_neurons_len = self.neurons.len();
                    let dst_neurons_len = layer.neurons.len();
                    for dst_idx in 0..dst_neurons_len {
                        for src_idx in 0..src_neurons_len {
                            let mut w_src_neuron = self
                                .neurons
                                .get(src_idx)
                                .expect("Cannot access to neuron by invalid index")
                                .write()
                                .await;
                            let r_dst_neuron = layer
                                .neurons
                                .get(dst_idx)
                                .expect("Cannot access to neuron by invalid index")
                                .read()
                                .await;

                            let mut rx = w_src_neuron.downlink_request();

                            r_dst_neuron
                                .connect(
                                    rx,
                                    *weights.get(dst_idx).unwrap_or(&1),
                                    w_src_neuron.id.clone(),
                                )
                                .await;
                        }
                    }
                }
                Layer::Output(layer) => {
                    let number_of_links = std::cmp::min(self.neurons.len(), layer.outputs.len());
                    for index in 0..number_of_links {
                        let mut w_neuron = self
                            .neurons
                            .get(index)
                            .expect("Cannot access to neuron by invalid index")
                            .write()
                            .await;
                        let mut w_output = layer
                            .outputs
                            .get(index)
                            .expect("Cannot access to output port by invalid index")
                            .write()
                            .await;
                        let rx = w_neuron.downlink_request();
                        w_output.connect(rx);
                    }
                }
                Layer::Input(_) => {
                    panic!(
                        "Its not possible to establish link downlink from hidden to input layers"
                    );
                }
            }
        }
    }

    struct OutputPort {
        id: Arc<String>,
        receiver: Option<broadcast::Receiver<u8>>,
    }

    impl OutputPort {
        fn new(id: usize) -> Self {
            Self {
                id: Arc::new(format!("O_{}", id)),
                receiver: None,
            }
        }

        fn connect(&mut self, rx: broadcast::Receiver<u8>) {
            if self.receiver.is_none() {
                self.receiver = Some(rx);
            } else {
                panic!("Port {} already connected", self.id);
            }
        }

        fn id(&self) -> &str {
            &self.id
        }

        fn disconnect(&mut self) {
            if self.receiver.is_some() {
                self.receiver = None;
            }
        }
    }

    pub struct OutputLayer {
        me: Weak<OutputLayer>,
        network: Weak<Network>,
        outputs: Vec<Arc<RwLock<OutputPort>>>,
    }

    impl OutputLayer {
        pub fn new(network: Weak<Network>, ports_number: usize) -> Arc<Self> {
            Arc::new_cyclic(|weak_self| Self {
                me: weak_self.clone(),
                network: network.clone(),
                outputs: (0..ports_number)
                    .map(|id| Arc::new(RwLock::new(OutputPort::new(id))))
                    .collect(),
            })
        }
    }

    pub enum Layer {
        Input(Arc<InputLayer>),
        Hidden(Arc<HiddenLayer>),
        Output(Arc<OutputLayer>),
    }
}

mod network {
    use std::{
        error::Error,
        sync::{Arc, Weak},
    };

    use cyclic_graph::CyclicGraph;
    use tokio::sync::RwLock;

    use crate::layouts::{InputLayer, Layer, OutputLayer};

    pub struct Network {
        me: Weak<Network>,
        content: CyclicGraph<String, Layer>,
    }

    impl Network {
        pub async fn new(
            input_ports_number: usize,
            output_ports_number: usize,
        ) -> Result<Arc<Self>, Box<dyn Error>> {
            let net = Arc::new_cyclic(|weak_network| {
                let input_layer =
                    Layer::Input(InputLayer::new(weak_network.clone(), input_ports_number));

                let output_layer =
                    Layer::Output(OutputLayer::new(weak_network.clone(), output_ports_number));
                Self {
                    me: weak_network.clone(),
                    content: CyclicGraph::new_default(
                        String::from("IL"),
                        input_layer,
                        String::from("OL"),
                        output_layer,
                        0_usize,
                    )
                    .expect("Cannot create cyclic graph instance"),
                }
            });
            Ok(net)
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    Ok(())
}
