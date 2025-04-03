use std::{
    collections::HashMap,
    error::Error,
    sync::{Arc, atomic::AtomicUsize},
};

use tokio::sync::RwLock;

use crate::{error::CyclicGraphError, node::Node};

pub struct CyclicGraph<T> {
    input: Arc<RwLock<Node<T>>>,
    output: Arc<RwLock<Node<T>>>,
    nodes: HashMap<usize, Arc<RwLock<Node<T>>>>,
    id_node_counter: AtomicUsize,
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
            id_node_counter: AtomicUsize::new(2),
        }
    }

    pub async fn add_node(
        &mut self,
        data: T,
        parents: &[usize],
        children: &[usize],
    ) -> Result<(), Box<dyn Error>> {
        if children.iter().any(|id| id == &0) {
            return Err(Box::new(CyclicGraphError::InsertBeforeInput));
        } else if parents.iter().any(|id| id == &1) {
            return Err(Box::new(CyclicGraphError::InsertAfterOutput));
        } else {
            let id = self
                .id_node_counter
                .fetch_add(1, std::sync::atomic::Ordering::Release);
            let new_node = Node::build(id, data);
            self.nodes.insert(id, new_node.clone());

            let mut w_new_node = new_node.write().await;

            for parent_id in parents {
                if let Some(parent) = self.nodes.get(parent_id) {
                    parent.write().await.add_child(new_node.clone()).await;
                    w_new_node.add_parent(parent.clone()).await;
                } else {
                    return Err(Box::new(CyclicGraphError::NodeNotFoundById(*parent_id)));
                }
            }

            for child_id in children {
                if let Some(child) = self.nodes.get(child_id) {
                    child.write().await.add_parent(new_node.clone()).await;
                    w_new_node.add_child(child.clone()).await;
                } else {
                    return Err(Box::new(CyclicGraphError::NodeNotFoundById(*child_id)));
                }
            }
        }

        Ok(())
    }

    pub async fn insert_between(
        &mut self,
        data: T,
        parent_id: usize,
        child_id: usize,
    ) -> Result<(), Box<dyn Error>> {
        if child_id == 0 {
            return Err(Box::new(CyclicGraphError::InsertBeforeInput));
        } else if parent_id == 1 {
            return Err(Box::new(CyclicGraphError::InsertAfterOutput));
        } else {
            let parent = self
                .nodes
                .get(&parent_id)
                .ok_or(Box::new(CyclicGraphError::NodeNotFoundById(parent_id)))?
                .clone();
            let child = self
                .nodes
                .get(&child_id)
                .ok_or(Box::new(CyclicGraphError::NodeNotFoundById(child_id)))?
                .clone();

            let id = self
                .id_node_counter
                .fetch_add(1, std::sync::atomic::Ordering::Release);
            let new_node = Node::build(id, data);
            self.nodes.insert(id, new_node.clone());

            let mut w_parent = parent.write().await;
            let mut w_child = child.write().await;
            let mut w_new_node = new_node.write().await;
            if w_parent.has_child(child_id) {
                w_parent.remove_child(child_id);
                w_child.remove_parent(parent_id);
            }
            w_parent.add_child(new_node.clone()).await;
            w_new_node.add_parent(parent.clone()).await;

            w_new_node.add_child(child.clone()).await;
            w_child.add_parent(new_node.clone()).await;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_new_can_create_cyclic_graph() {
        let graph = CyclicGraph::new("input_data", "output_data");

        assert_eq!(graph.input.read().await.id(), 0);
        assert_eq!(graph.output.read().await.id(), 1);
        assert_eq!(graph.nodes.len(), 2);
    }
}
