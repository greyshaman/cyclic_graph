use std::{
    collections::HashMap,
    error::Error,
    sync::{Arc, atomic::AtomicUsize},
};

use tokio::sync::RwLock;

use crate::{error::CyclicGraphError, node::Node};

pub struct CyclicGraph<T> {
    input: Arc<Node<T>>,
    output: Arc<Node<T>>,
    nodes: HashMap<usize, Arc<Node<T>>>,
    id_node_counter: AtomicUsize,
}

impl<T> CyclicGraph<T> {
    pub async fn new(input_data: T, output_data: T) -> Self {
        let input = Arc::new(Node::new(0, input_data));
        let output = Arc::new(Node::new(1, output_data));

        let mut nodes = HashMap::new();
        nodes.insert(0, input.clone());
        nodes.insert(1, output.clone());

        input.link_child(output.clone()).await;

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
        parent_ids: &[usize],
        child_ids: &[usize],
    ) -> Result<Arc<Node<T>>, Box<dyn Error>> {
        if child_ids.iter().any(|id| id == &0) {
            return Err(Box::new(CyclicGraphError::InsertBeforeInput));
        } else if parent_ids.iter().any(|id| id == &1) {
            return Err(Box::new(CyclicGraphError::InsertAfterOutput));
        }

        let id = self
            .id_node_counter
            .fetch_add(1, std::sync::atomic::Ordering::Release);
        let new_node = Arc::new(Node::new(id, data));
        self.nodes.insert(id, new_node.clone());

        for parent_id in parent_ids {
            if let Some(parent) = self.nodes.get(parent_id) {
                parent.link_child(new_node.clone()).await;
                new_node.link_parent(parent.clone()).await;
            } else {
                return Err(Box::new(CyclicGraphError::NodeNotFoundById(*parent_id)));
            }
        }

        for child_id in child_ids {
            if let Some(child) = self.nodes.get(child_id) {
                child.link_parent(new_node.clone()).await;
                new_node.link_child(child.clone()).await;
            } else {
                return Err(Box::new(CyclicGraphError::NodeNotFoundById(*child_id)));
            }
        }

        Ok(new_node.clone())
    }

    pub async fn insert_between(
        &mut self,
        data: T,
        parent_id: usize,
        child_id: usize,
    ) -> Result<Arc<Node<T>>, Box<dyn Error>> {
        if child_id == 0 {
            return Err(Box::new(CyclicGraphError::InsertBeforeInput));
        } else if parent_id == 1 {
            return Err(Box::new(CyclicGraphError::InsertAfterOutput));
        }

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
        let new_node = Arc::new(Node::new(id, data));
        self.nodes.insert(id, new_node.clone());

        if parent.has_child(child_id).await {
            parent.unlink_child(child.clone()).await;
        }

        parent.link_child(new_node.clone()).await;
        child.link_parent(new_node.clone()).await;

        Ok(new_node.clone())
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_new_can_create_cyclic_graph() {
        let graph = CyclicGraph::new("input_data", "output_data").await;

        assert_eq!(graph.input.id(), 0);
        assert_eq!(graph.output.id(), 1);
        assert_eq!(graph.len(), 2);

        assert!(graph.input.has_child(1).await);
        assert!(graph.output.has_parent(0).await);
    }

    #[tokio::test]
    async fn test_add_node_can_add_new_node_to_empty_graph() -> Result<(), Box<dyn Error>> {
        let mut graph = CyclicGraph::new("input", "output").await;
        let new_node = graph.add_node("hidden", &[0], &[1]).await?;

        assert_eq!(graph.len(), 3);
        assert_eq!(new_node.id(), 2);
        assert!(new_node.has_parent(0).await);
        assert!(new_node.has_child(1).await);

        assert!(graph.input.has_child(1).await);
        assert!(graph.input.has_child(2).await);
        assert!(graph.output.has_parent(0).await);
        assert!(graph.output.has_parent(2).await);

        Ok(())
    }

    #[tokio::test]
    async fn test_add_node_should_return_error_when_input_id_in_children_param() {
        let mut graph = CyclicGraph::new("input", "output").await;
        let result = graph.add_node("hidden", &[0], &[0]).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_add_node_should_return_error_when_output_id_in_parent_param() {
        let mut graph = CyclicGraph::new("input", "output").await;
        let result = graph.add_node("hidden", &[1], &[1]).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_insert_between_create_and_inset_new_node_between_specified_nodes()
    -> Result<(), Box<dyn Error>> {
        let mut graph = CyclicGraph::new("input_data", "output_data").await;
        let result = graph.insert_between("middle", 0, 1).await;

        assert_eq!(graph.len(), 3);
        assert!(result.is_ok());

        let new_node = result.unwrap();
        assert_eq!(new_node.id(), 2);
        assert!(new_node.has_parent(0).await);
        assert!(new_node.has_child(1).await);

        assert!(!graph.input.has_child(1).await);
        assert!(graph.input.has_child(2).await);
        assert!(!graph.output.has_parent(0).await);
        assert!(graph.output.has_parent(2).await);

        Ok(())
    }
}
