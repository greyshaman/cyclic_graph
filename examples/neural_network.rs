use std::{
    error::Error,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
};

use cyclic_graph::Content;
use tokio_stream::StreamExt;

use crate::{
    layouts::{hidden::HiddenLayer, input::InputLayer, output::OutputLayer},
    network::Network,
};

mod layouts;

mod network {
    use std::{error::Error, sync::Arc};

    use cyclic_graph::{Content, CyclicGraph};

    use crate::layouts::{input::InputLayer, output::OutputLayer};

    pub struct Network {
        id: String,
        content: Arc<CyclicGraph<String, (), u8>>,
    }

    impl Network {
        pub fn new(
            id: usize,
            input_ports_number: usize,
            output_ports_number: usize,
        ) -> Result<Arc<Self>, Box<dyn Error>> {
            let net_id = format!("N{id}");
            let input_layer = InputLayer::new(&net_id, input_ports_number);
            let output_layer = OutputLayer::new(&net_id, output_ports_number);
            let net = Self {
                id: net_id,
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
            };
            let net = Arc::new(net);
            Ok(net)
        }

        pub fn id(&self) -> String {
            self.id.clone()
        }

        pub fn content(&self) -> Arc<CyclicGraph<String, (), u8>> {
            self.content.clone()
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Create the network
    let net = Network::new(0, 3, 8)?;

    // Insert the hidden layer with 3 neurons between the input and output layers
    let hidden_node_0 = net
        .content()
        .insert_between(
            Content::new_layer(HiddenLayer::new(0, &net.id(), 3, &vec![1, 2, 3])),
            "IL".to_string(),
            "OL".to_string(),
        )
        .await?;

    // Insert the hidden layer with 8 neurons between the hidden_node_0 and output layers
    let _hidden_node_1 = net
        .content()
        .insert_between(
            Content::new_layer(HiddenLayer::new(
                1,
                &net.id(),
                8,
                &vec![1, 2, 3, 4, 4, 3, 2, 1],
            )),
            hidden_node_0.id().to_string(),
            "OL".to_string(),
        )
        .await?;

    // Create listener from output stream
    {
        let net = net.clone();
        let output_layer = net
            .content()
            .output()
            .content()
            .as_layer()
            .expect("Cannot get output layer")
            .clone();

        tokio::spawn(async move {
            let output_layer = output_layer
                .as_any()
                .downcast_ref::<OutputLayer>()
                .expect("Cannot downcast to output layer");
            let mut stream = output_layer.into_stream().await;
            while let Some(output) = stream.next().await {
                println!("Channel {} has received signal: {}", output.0, output.1);
            }
        });
    }

    // Generate test signals and inject them into the network
    // Inject signal
    let input_content = net
        .clone()
        .content()
        .input()
        .clone()
        .content()
        .as_layer()
        .expect("Cannot get input layer");

    let mut tasks = vec![];

    let signal_gen = Arc::new(AtomicU8::new(0));

    for j in 0_u8..10 {
        let input_content = input_content.clone();
        let signal_gen = signal_gen.clone();
        tasks.push(tokio::spawn(async move {
            let input_layer = input_content
                .as_any()
                .downcast_ref::<InputLayer>()
                .expect("Cannot downcast to input layer");

            let port_ids: Vec<String> = {
                let ports = input_layer.port_ids().await;
                ports.iter().map(|id| id.clone()).collect()
            };

            if port_ids.is_empty() {
                return;
            }

            for _ in 0..3 {
                let port_idx = ((j + 1) as usize) % port_ids.len();
                let port_id = port_ids[port_idx].clone();

                if let Err(e) = input_layer
                    .send_to(signal_gen.fetch_add(1, Ordering::Release), port_id.clone())
                    .await
                {
                    eprintln!("Failed to send to port {}: {}", port_id, e)
                }
            }
        }));
    }

    for task in tasks {
        task.await.unwrap();
    }
    Ok(())
}
