use std::{
    collections::{HashMap, HashSet},
    error::Error,
    fmt::{Debug, Display},
    hash::Hash,
    sync::{Arc, atomic::AtomicUsize},
};

use async_recursion::async_recursion;
use tokio::sync::RwLock;

use crate::{error::CyclicGraphError, node::Node};

pub enum GeneratorMode {
    Normal,
    DryRun,
}

pub struct CyclicGraph<I, T, G>
where
    G: Fn(&AtomicUsize, GeneratorMode) -> I,
{
    input: Arc<Node<I, T>>,
    output: Arc<Node<I, T>>,
    nodes: HashMap<I, Arc<Node<I, T>>>,
    id_generator: G,
    recent_id: AtomicUsize,
}

impl<I, T, G> CyclicGraph<I, T, G>
where
    I: Clone + Eq + Hash + Debug + Display + Sync + Send + 'static,
    T: Send + Sync,
    G: Fn(&AtomicUsize, GeneratorMode) -> I + Sync + Send,
{
    pub async fn new(
        input_id: I,
        input_data: T,
        output_id: I,
        output_data: T,
        start_id_idx: usize,
        id_generator: G,
    ) -> Result<Self, Box<dyn Error>> {
        if input_id == output_id {
            return Err(Box::new(CyclicGraphError::NonUniqueId(output_id.clone())));
        }

        let input = Arc::new(Node::new(input_id, input_data));
        let output = Arc::new(Node::new(output_id, output_data));

        let mut nodes = HashMap::new();
        nodes.insert(input.id().clone(), input.clone());
        nodes.insert(output.id().clone(), output.clone());

        input.link_child(output.clone()).await;

        let recent_id = AtomicUsize::new(start_id_idx);
        let try_id = id_generator(&recent_id, GeneratorMode::DryRun);

        if input.id() == &try_id || output.id() == &try_id {
            return Err(Box::new(CyclicGraphError::NonUniqueId(try_id.clone())));
        }

        Ok(Self {
            input,
            output,
            nodes,
            id_generator,
            recent_id,
        })
    }

    pub async fn append_node(
        &mut self,
        data: T,
        parent_ids: &[I],
        child_ids: &[I],
    ) -> Result<Arc<Node<I, T>>, Box<dyn Error>> {
        if child_ids.iter().any(|id| id == self.input.id()) {
            return Err(Box::new(CyclicGraphError::InsertBeforeInput::<I>));
        } else if parent_ids.iter().any(|id| id == self.output.id()) {
            return Err(Box::new(CyclicGraphError::InsertAfterOutput::<I>));
        }

        let id = (self.id_generator)(&self.recent_id, GeneratorMode::Normal);
        let new_node = Arc::new(Node::new(id.clone(), data));
        self.nodes.insert(id, new_node.clone());

        for parent_id in parent_ids {
            if let Some(parent) = self.nodes.get(parent_id) {
                parent.link_child(new_node.clone()).await;
                new_node.link_parent(parent.clone()).await;
            } else {
                return Err(Box::new(CyclicGraphError::NodeNotFoundById(
                    parent_id.clone(),
                )));
            }
        }

        for child_id in child_ids {
            if let Some(child) = self.nodes.get(child_id) {
                child.link_parent(new_node.clone()).await;
                new_node.link_child(child.clone()).await;
            } else {
                return Err(Box::new(CyclicGraphError::NodeNotFoundById(
                    child_id.clone(),
                )));
            }
        }

        Ok(new_node.clone())
    }

    pub async fn insert_between(
        &mut self,
        data: T,
        parent_id: I,
        child_id: I,
    ) -> Result<Arc<Node<I, T>>, Box<dyn Error>> {
        if &child_id == self.input.id() {
            return Err(Box::new(CyclicGraphError::InsertBeforeInput::<I>));
        } else if &parent_id == self.output.id() {
            return Err(Box::new(CyclicGraphError::InsertAfterOutput::<I>));
        }

        let parent = self
            .nodes
            .get(&parent_id)
            .ok_or(Box::new(CyclicGraphError::NodeNotFoundById(
                parent_id.clone(),
            )))?
            .clone();
        let child = self
            .nodes
            .get(&child_id)
            .ok_or(Box::new(CyclicGraphError::NodeNotFoundById(
                child_id.clone(),
            )))?
            .clone();

        let id = (self.id_generator)(&self.recent_id, GeneratorMode::Normal);
        let new_node = Arc::new(Node::new(id.clone(), data));
        self.nodes.insert(id, new_node.clone());

        if parent.has_child(&child_id).await {
            parent.unlink_child(child.clone()).await;
        }

        parent.link_child(new_node.clone()).await;
        child.link_parent(new_node.clone()).await;

        Ok(new_node.clone())
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub async fn traverse_from_input(&self) -> Vec<I> {
        let visited = Arc::new(RwLock::new(HashSet::<I>::new()));
        let result = Arc::new(RwLock::new(Vec::<I>::new()));
        self.dfs(self.input.clone(), visited.clone(), result.clone())
            .await;
        result.read().await.clone()
    }

    #[async_recursion]
    async fn dfs(
        &self,
        node: Arc<Node<I, T>>,
        visited: Arc<RwLock<HashSet<I>>>,
        result: Arc<RwLock<Vec<I>>>,
    ) {
        let children_ids = node.child_ids().await;

        for child in children_ids.iter().filter_map(|id| self.nodes.get(id)) {
            let child_id = child.id();
            if visited.write().await.insert(child_id.clone()) {
                result.write().await.push(child_id.clone());
                self.dfs(child.clone(), visited.clone(), result.clone())
                    .await;
            }
        }
    }

    async fn bfs(&self, from_node: Arc<Node<I, T>>, goal_node: Arc<Node<I, T>>) -> bool {
        let mut visited = HashSet::<I>::new();
        let mut queue = Vec::<Arc<Node<I, T>>>::new();

        queue.push(from_node.clone());
        while !queue.is_empty() {
            let node = queue.pop().expect("queue is not empty");
            if node.id() == goal_node.id() {
                return true;
            }
            visited.insert(node.id().clone());

            let ids = node.child_ids().await;
            for child in ids.iter().filter_map(|id| self.nodes.get(id)) {
                let id = child.id();
                if visited.insert(id.clone()) {
                    queue.push(child.clone());
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod for_id_as_usize {
        use super::*;

        #[tokio::test]
        async fn test_new_can_create_cyclic_graph() -> Result<(), Box<dyn Error>> {
            let graph = CyclicGraph::new(
                0,
                "input_data",
                1,
                "output_data",
                2,
                |recent_id, mode| match mode {
                    GeneratorMode::Normal => {
                        recent_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                    }
                    GeneratorMode::DryRun => recent_id.load(std::sync::atomic::Ordering::Relaxed),
                },
            )
            .await?;

            assert_eq!(graph.input.id(), &0);
            assert_eq!(graph.output.id(), &1);
            assert_eq!(graph.len(), 2);

            assert!(graph.input.has_child(&1).await);
            assert!(graph.output.has_parent(&0).await);

            Ok(())
        }

        #[tokio::test]
        async fn test_new_should_return_error_when_terminal_nodes_has_same_ids() {
            let result = CyclicGraph::new(
                0,
                "input_data",
                0,
                "output_data",
                2,
                |recent_id, mode| match mode {
                    GeneratorMode::Normal => {
                        recent_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                    }
                    GeneratorMode::DryRun => recent_id.load(std::sync::atomic::Ordering::Relaxed),
                },
            )
            .await;

            assert!(result.is_err());
        }

        #[tokio::test]
        async fn test_new_should_return_error_when_start_id_idx_same_of_input_node() {
            let result = CyclicGraph::new(
                0,
                "input_data",
                1,
                "output_data",
                0,
                |recent_id, mode| match mode {
                    GeneratorMode::Normal => {
                        recent_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                    }
                    GeneratorMode::DryRun => recent_id.load(std::sync::atomic::Ordering::Relaxed),
                },
            )
            .await;

            assert!(result.is_err());
        }

        #[tokio::test]
        async fn test_new_should_return_error_when_start_id_idx_same_of_output_node() {
            let result = CyclicGraph::new(
                0,
                "input_data",
                1,
                "output_data",
                1,
                |recent_id, mode| match mode {
                    GeneratorMode::Normal => {
                        recent_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                    }
                    GeneratorMode::DryRun => recent_id.load(std::sync::atomic::Ordering::Relaxed),
                },
            )
            .await;

            assert!(result.is_err());
        }

        #[tokio::test]
        async fn test_add_node_can_add_new_node_to_empty_graph() -> Result<(), Box<dyn Error>> {
            let mut graph =
                CyclicGraph::new(0, "input", 1, "output", 2, |recent_id, mode| match mode {
                    GeneratorMode::Normal => {
                        recent_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                    }
                    GeneratorMode::DryRun => recent_id.load(std::sync::atomic::Ordering::Relaxed),
                })
                .await?;
            let new_node = graph.append_node("hidden", &[0], &[1]).await?;

            assert_eq!(graph.len(), 3);
            assert_eq!(new_node.id(), &2);
            assert!(new_node.has_parent(&0).await);
            assert!(new_node.has_child(&1).await);

            assert!(graph.input.has_child(&1).await);
            assert!(graph.input.has_child(&2).await);
            assert!(graph.output.has_parent(&0).await);
            assert!(graph.output.has_parent(&2).await);

            Ok(())
        }

        #[tokio::test]
        async fn test_add_node_should_return_error_when_input_id_in_children_param() {
            let mut graph =
                CyclicGraph::new(0, "input", 1, "output", 2, |recent_id, mode| match mode {
                    GeneratorMode::Normal => {
                        recent_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                    }
                    GeneratorMode::DryRun => recent_id.load(std::sync::atomic::Ordering::Relaxed),
                })
                .await
                .unwrap();
            let result = graph.append_node("hidden", &[0], &[0]).await;

            assert!(result.is_err());
        }

        #[tokio::test]
        async fn test_add_node_should_return_error_when_output_id_in_parent_param() {
            let mut graph =
                CyclicGraph::new(0, "input", 1, "output", 2, |recent_id, mode| match mode {
                    GeneratorMode::Normal => {
                        recent_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                    }
                    GeneratorMode::DryRun => recent_id.load(std::sync::atomic::Ordering::Relaxed),
                })
                .await
                .unwrap();
            let result = graph.append_node("hidden", &[1], &[1]).await;

            assert!(result.is_err());
        }

        #[tokio::test]
        async fn test_insert_between_create_and_inset_new_node_between_specified_nodes()
        -> Result<(), Box<dyn Error>> {
            let mut graph = CyclicGraph::new(
                0,
                "input_data",
                1,
                "output_data",
                2,
                |recent_id, mode| match mode {
                    GeneratorMode::Normal => {
                        recent_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                    }
                    GeneratorMode::DryRun => recent_id.load(std::sync::atomic::Ordering::Relaxed),
                },
            )
            .await
            .unwrap();
            let result = graph.insert_between("middle", 0, 1).await;

            assert_eq!(graph.len(), 3);
            assert!(result.is_ok());

            let new_node = result.unwrap();
            assert_eq!(new_node.id(), &2);
            assert!(new_node.has_parent(&0).await);
            assert!(new_node.has_child(&1).await);

            assert!(!graph.input.has_child(&1).await);
            assert!(graph.input.has_child(&2).await);
            assert!(!graph.output.has_parent(&0).await);
            assert!(graph.output.has_parent(&2).await);

            Ok(())
        }
    }
}
