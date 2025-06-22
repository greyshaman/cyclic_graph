use std::error::Error;

use cyclic_graph::Content;

use crate::{layouts::hidden::HiddenLayer, network::Network};

mod layouts;

mod network {
    use std::{
        error::Error,
        sync::{Arc, Weak},
    };

    use cyclic_graph::{Content, CyclicGraph};

    use crate::layouts::{input::InputLayer, output::OutputLayer};

    pub struct Network {
        me: Weak<Network>,
        id: String,
        content: Arc<CyclicGraph<String, (), u8>>,
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
                    content: Arc::new(
                        CyclicGraph::new_default(
                            String::from("IL"),
                            Content::new_layer(input_layer),
                            String::from("OL"),
                            Content::new_layer(output_layer),
                            0_usize,
                        )
                        .expect("Cannot create cyclic graph instance"),
                    ),
                }
            });
            Ok(net)
        }

        pub fn id(&self) -> String {
            self.id.clone()
        }

        pub fn content(&self) -> Arc<CyclicGraph<String, (), u8>> {
            self.content.clone()
        }

        pub fn me(&self) -> Weak<Network> {
            self.me.clone()
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let net = Network::new(0, 3, 8)?;

    let hidden_node_0 = net
        .content()
        .insert_between(
            Content::new_layer(HiddenLayer::new(0, net.me(), 3, &vec![1, 2, 3])),
            "IL".to_string(),
            "OL".to_string(),
        )
        .await?;

    net.content()
        .insert_between(
            Content::new_layer(HiddenLayer::new(
                1,
                net.me(),
                8,
                &vec![1, 2, 3, 4, 4, 3, 2, 1],
            )),
            hidden_node_0.id().to_string(),
            "OL".to_string(),
        )
        .await?;

    Ok(())
}
