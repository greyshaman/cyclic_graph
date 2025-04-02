use std::{
    collections::HashMap,
    sync::{Arc, Weak},
};

use node::Node;
use tokio::sync::RwLock;

mod node;

pub struct CyclicGraph<T> {
    input: Arc<RwLock<Node<T>>>,
    output: Arc<RwLock<Node<T>>>,
    nodes: HashMap<u64, Arc<RwLock<Node<T>>>>,
}

impl<T> CyclicGraph<T> {
    pub fn new(input_data: T, output_data: T) -> Self {
        let input = Arc::new(RwLock::new(Node::new(0, input_data)));

        let output = Arc::new(RwLock::new(Node::new(1, output_data)));

        let mut nodes = HashMap::new();
        nodes.insert(0, input.clone());
        nodes.insert(1, output.clone());

        Self {
            input,
            output,
            nodes,
        }
    }
}
