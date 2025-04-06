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
    G: Fn(&AtomicUsize, GeneratorMode) -> I + Sync + Send + 'static,
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

    pub async fn traverse_from_input_node(&self) -> Vec<I> {
        let visited = Arc::new(RwLock::new(HashSet::<I>::new()));
        let result = Arc::new(RwLock::new(Vec::<I>::new()));
        result.write().await.push(self.input.id().clone());
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

    pub async fn bfs(&self, from_node: Arc<Node<I, T>>, goal_node: Arc<Node<I, T>>) -> bool {
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
        async fn test_append_node_can_add_new_node_to_empty_graph() -> Result<(), Box<dyn Error>> {
            let mut graph =
                CyclicGraph::new(0, "input", 1, "output", 2, |recent_id, mode| match mode {
                    GeneratorMode::Normal => {
                        recent_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                    }
                    GeneratorMode::DryRun => recent_id.load(std::sync::atomic::Ordering::Relaxed),
                })
                .await?;
            let n2 = graph.append_node("hidden2", &[0], &[1]).await?;
            let n3 = graph.append_node("hidden3", &[0], &[1]).await?;

            assert_eq!(graph.len(), 4);
            assert_eq!(n2.id(), &2);
            assert_eq!(n3.id(), &3);
            assert!(n2.has_parent(&0).await);
            assert!(n2.has_child(&1).await);

            assert!(n3.has_parent(&0).await);
            assert!(n3.has_child(&1).await);

            assert!(graph.input.has_child(&1).await);
            assert!(graph.input.has_child(&2).await);
            assert!(graph.input.has_child(&3).await);
            assert!(graph.output.has_parent(&0).await);
            assert!(graph.output.has_parent(&2).await);
            assert!(graph.output.has_parent(&3).await);

            Ok(())
        }

        #[tokio::test]
        async fn test_append_node_should_return_error_when_input_id_in_children_param() {
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
        async fn test_append_node_should_return_error_when_output_id_in_parent_param() {
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
        async fn test_serial_insert_between_create_and_inset_new_nodes_between_specified_nodes()
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
            assert!(result.is_ok());
            let n2 = result.unwrap();

            let n3 = graph.insert_between("middle", 2, 1).await?;

            // input -> n2 -> n3 -> output
            assert_eq!(graph.len(), 4);

            assert_eq!(n2.id(), &2);
            assert_eq!(n3.id(), &3);

            assert!(n2.has_parent(&0).await);
            assert!(n2.has_child(&3).await);

            assert!(n3.has_parent(&2).await);
            assert!(n3.has_child(&1).await);

            assert!(!graph.input.has_child(&1).await);
            assert!(graph.input.has_child(&2).await);
            assert!(!graph.input.has_child(&3).await);
            assert!(!graph.output.has_parent(&0).await);
            assert!(!graph.output.has_parent(&2).await);
            assert!(graph.output.has_parent(&3).await);

            Ok(())
        }

        #[tokio::test]
        async fn test_parallel_insert_between_create_and_inset_new_nodes_between_specified_nodes()
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
            .await?;

            let result = graph.insert_between("middle2", 0, 1).await;
            assert!(result.is_ok());
            let n2 = result.unwrap();

            let n3 = graph.insert_between("middle3", 0, 1).await?;

            // input -> [n2, n3] -> output
            assert_eq!(graph.len(), 4);

            assert_eq!(n2.id(), &2);
            assert_eq!(n3.id(), &3);

            assert!(n2.has_parent(&0).await);
            assert!(n2.has_child(&1).await);

            assert!(n3.has_parent(&0).await);
            assert!(n3.has_child(&1).await);

            assert!(!graph.input.has_child(&1).await);
            assert!(graph.input.has_child(&2).await);
            assert!(graph.input.has_child(&3).await);

            assert!(!graph.output.has_parent(&0).await);
            assert!(graph.output.has_parent(&2).await);
            assert!(graph.output.has_parent(&3).await);

            Ok(())
        }

        #[tokio::test]
        async fn test_traverse_from_input_should_return_correct_path_serial_graph()
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
            .await?;

            let n2 = graph.insert_between("middle2", 0, 1).await?;

            let _n3 = graph.insert_between("middle3", n2.id().clone(), 1).await?;

            // input -> n2 -> n3 -> output
            assert_eq!(graph.len(), 4);

            let path = graph.traverse_from_input_node().await;
            dbg!(&path);

            assert_eq!(path.get(0), Some(&0));
            assert_eq!(path.get(1), Some(&2));
            assert_eq!(path.get(2), Some(&3));
            assert_eq!(path.get(3), Some(&1));

            Ok(())
        }

        #[tokio::test]
        async fn test_traverse_from_input_should_return_correct_path_parallel_graph()
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
            .await?;

            let _n2 = graph.insert_between("middle2", 0, 1).await?;

            let _n3 = graph.insert_between("middle3", 0, 1).await?;

            // input -> [n2, n3] -> output
            assert_eq!(graph.len(), 4);

            let path = graph.traverse_from_input_node().await;

            assert_eq!(path.len(), 4);

            Ok(())
        }
    }

    mod for_id_as_string {
        use super::*;

        #[tokio::test]
        async fn test_new_can_create_cyclic_graph() -> Result<(), Box<dyn Error>> {
            let graph = CyclicGraph::new(
                String::from("IL"),
                "input_data",
                String::from("OL"),
                "output_data",
                0,
                |recent_id, mode| match mode {
                    GeneratorMode::Normal => {
                        format!(
                            "ML_{}",
                            recent_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                        )
                    }
                    GeneratorMode::DryRun => format!(
                        "ML_{}",
                        recent_id.load(std::sync::atomic::Ordering::Relaxed)
                    ),
                },
            )
            .await?;

            assert_eq!(graph.input.id(), "IL");
            assert_eq!(graph.output.id(), "OL");
            assert_eq!(graph.len(), 2);

            assert!(graph.input.has_children().await);
            assert!(graph.input.has_child("OL").await);
            assert!(graph.output.has_parents().await);
            assert!(graph.output.has_parent("IL").await);

            Ok(())
        }

        #[tokio::test]
        async fn test_new_should_return_error_when_terminal_nodes_has_same_ids() {
            let result = CyclicGraph::new(
                String::from("IL"),
                "input_data",
                String::from("IL"),
                "output_data",
                1,
                |recent_id, mode| match mode {
                    GeneratorMode::Normal => {
                        format!(
                            "ML_{}",
                            recent_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                        )
                    }
                    GeneratorMode::DryRun => format!(
                        "ML_{}",
                        recent_id.load(std::sync::atomic::Ordering::Relaxed)
                    ),
                },
            )
            .await;

            assert!(result.is_err());
        }

        #[tokio::test]
        async fn test_append_node_should_add_new_nodes_with_correct_ids()
        -> Result<(), Box<dyn Error>> {
            let mut graph = CyclicGraph::new(
                String::from("IL"),
                "input_data",
                String::from("OL"),
                "output_data",
                0,
                |recent_id, mode| match mode {
                    GeneratorMode::Normal => {
                        format!(
                            "ML_{}",
                            recent_id.fetch_add(1, std::sync::atomic::Ordering::Release)
                        )
                    }
                    GeneratorMode::DryRun => format!(
                        "ML_{}",
                        recent_id.load(std::sync::atomic::Ordering::Acquire)
                    ),
                },
            )
            .await?;

            let new_node = graph
                .append_node("hidden", &["IL".to_string()], &["OL".to_string()])
                .await?;

            let new_node2 = graph
                .append_node(
                    "hidden2",
                    &["IL".to_string(), new_node.id().into()],
                    &["OL".to_string()],
                )
                .await?;

            assert_eq!(graph.len(), 4);
            assert_eq!(new_node.id(), "ML_0");
            assert_eq!(new_node2.id(), "ML_1");

            assert!(new_node.has_parent("IL").await);
            assert!(new_node.has_child("OL").await);

            assert!(new_node2.has_parent("IL").await);
            assert!(new_node2.has_parent("ML_0").await);
            assert!(new_node2.has_child("OL").await);

            assert!(graph.input.has_child("OL").await);
            assert!(graph.input.has_child("ML_0").await);
            assert!(graph.input.has_child("ML_1").await);
            assert!(graph.output.has_parent("IL").await);
            assert!(graph.output.has_parent("ML_0").await);
            assert!(graph.output.has_parent("ML_1").await);

            Ok(())
        }

        #[tokio::test]
        async fn test_append_node_should_return_error_when_input_id_in_children_param() {
            let mut graph = CyclicGraph::new(
                String::from("IL"),
                "input",
                String::from("OL"),
                "output",
                0,
                |recent_id, mode| match mode {
                    GeneratorMode::Normal => {
                        format!(
                            "ML_{}",
                            recent_id.fetch_add(1, std::sync::atomic::Ordering::Release)
                        )
                    }
                    GeneratorMode::DryRun => format!(
                        "ML_{}",
                        recent_id.load(std::sync::atomic::Ordering::Acquire)
                    ),
                },
            )
            .await
            .unwrap();
            let result = graph
                .append_node("hidden", &[String::from("IL")], &[String::from("IL")])
                .await;

            assert!(result.is_err());
        }

        #[tokio::test]
        async fn test_append_node_should_return_error_when_output_id_in_parent_param() {
            let mut graph = CyclicGraph::new(
                String::from("IL"),
                "input",
                String::from("OL"),
                "output",
                0,
                |recent_id, mode| match mode {
                    GeneratorMode::Normal => {
                        format!(
                            "ML_{}",
                            recent_id.fetch_add(1, std::sync::atomic::Ordering::Release)
                        )
                    }
                    GeneratorMode::DryRun => format!(
                        "ML_{}",
                        recent_id.load(std::sync::atomic::Ordering::Acquire)
                    ),
                },
            )
            .await
            .unwrap();
            let result = graph
                .append_node("hidden", &[String::from("OL")], &[String::from("OL")])
                .await;

            assert!(result.is_err());
        }

        #[tokio::test]
        async fn test_serial_insert_between_create_and_inset_new_nodes_between_specified_nodes()
        -> Result<(), Box<dyn Error>> {
            let mut graph = CyclicGraph::new(
                String::from("IL"),
                "input_data",
                String::from("OL"),
                "output_data",
                0,
                |recent_id, mode| match mode {
                    GeneratorMode::Normal => {
                        format!(
                            "ML_{}",
                            recent_id.fetch_add(1, std::sync::atomic::Ordering::Release)
                        )
                    }
                    GeneratorMode::DryRun => format!(
                        "ML_{}",
                        recent_id.load(std::sync::atomic::Ordering::Acquire)
                    ),
                },
            )
            .await
            .unwrap();

            let result = graph
                .insert_between("middle", String::from("IL"), String::from("OL"))
                .await;
            assert!(result.is_ok());
            let n0 = result.unwrap();

            let n1 = graph
                .insert_between("middle", n0.id().into(), String::from("OL"))
                .await?;

            // input -> n0 -> n1 -> output
            assert_eq!(graph.len(), 4);

            assert_eq!(n0.id(), "ML_0");
            assert!(n0.has_parent("IL").await);
            assert!(n0.has_child(n1.id()).await);

            assert_eq!(n1.id(), "ML_1");
            assert!(n1.has_parent("ML_0").await);
            assert!(n1.has_child("OL").await);

            assert!(!graph.input.has_child("OL").await);
            assert!(graph.input.has_child("ML_0").await);
            assert!(!graph.input.has_child("ML_2").await);
            assert!(!graph.output.has_parent("IL").await);
            assert!(!graph.output.has_parent("ML_0").await);
            assert!(graph.output.has_parent("ML_1").await);

            Ok(())
        }

        #[tokio::test]
        async fn test_parallel_insert_between_create_and_inset_new_nodes_between_specified_nodes()
        -> Result<(), Box<dyn Error>> {
            let mut graph = CyclicGraph::new(
                String::from("IL"),
                "input_data",
                String::from("OL"),
                "output_data",
                0,
                |recent_id, mode| match mode {
                    GeneratorMode::Normal => {
                        format!(
                            "ML_{}",
                            recent_id.fetch_add(1, std::sync::atomic::Ordering::Release)
                        )
                    }
                    GeneratorMode::DryRun => format!(
                        "ML_{}",
                        recent_id.load(std::sync::atomic::Ordering::Acquire)
                    ),
                },
            )
            .await
            .unwrap();

            let result = graph
                .insert_between("middle0", String::from("IL"), String::from("OL"))
                .await;
            assert!(result.is_ok());
            let n0 = result.unwrap();

            let n1 = graph
                .insert_between("middle1", graph.input.id().into(), graph.output.id().into())
                .await?;

            // input -> [n0, n1] -> output
            assert_eq!(graph.len(), 4);

            assert_eq!(n0.id(), "ML_0");
            assert!(n0.has_parent("IL").await);
            assert!(n0.has_child("OL").await);

            assert_eq!(n1.id(), "ML_1");
            assert!(n1.has_parent("IL").await);
            assert!(n1.has_child("OL").await);

            assert!(!graph.input.has_child("OL").await);
            assert!(graph.input.has_child("ML_0").await);
            assert!(graph.input.has_child("ML_1").await);
            assert!(!graph.output.has_parent("IL").await);
            assert!(graph.output.has_parent("ML_0").await);
            assert!(graph.output.has_parent("ML_1").await);

            Ok(())
        }

        #[tokio::test]
        async fn test_traverse_from_input_node_for_serial_graph() -> Result<(), Box<dyn Error>> {
            let mut graph = CyclicGraph::new(
                String::from("IL"),
                "input_data",
                String::from("OL"),
                "output_data",
                0,
                |recent_id, mode| match mode {
                    GeneratorMode::Normal => {
                        format!(
                            "ML_{}",
                            recent_id.fetch_add(1, std::sync::atomic::Ordering::Release)
                        )
                    }
                    GeneratorMode::DryRun => format!(
                        "ML_{}",
                        recent_id.load(std::sync::atomic::Ordering::Acquire)
                    ),
                },
            )
            .await?;

            let n0 = graph
                .insert_between("middle", String::from("IL"), String::from("OL"))
                .await?;

            let n1 = graph
                .insert_between("middle", n0.id().into(), String::from("OL"))
                .await?;

            // input -> n0 -> n1 -> output
            assert_eq!(graph.len(), 4);

            let path = graph.traverse_from_input_node().await;

            assert_eq!(path.get(0), Some(graph.input.id()));
            assert_eq!(path.get(1), Some(n0.id()));
            assert_eq!(path.get(2), Some(n1.id()));
            assert_eq!(path.get(3), Some(graph.output.id()));

            Ok(())
        }

        #[tokio::test]
        async fn test_traverse_from_input_node_for_parallel_graph() -> Result<(), Box<dyn Error>> {
            let mut graph = CyclicGraph::new(
                String::from("IL"),
                "input_data",
                String::from("OL"),
                "output_data",
                0,
                |recent_id, mode| match mode {
                    GeneratorMode::Normal => {
                        format!(
                            "ML_{}",
                            recent_id.fetch_add(1, std::sync::atomic::Ordering::Release)
                        )
                    }
                    GeneratorMode::DryRun => format!(
                        "ML_{}",
                        recent_id.load(std::sync::atomic::Ordering::Acquire)
                    ),
                },
            )
            .await?;

            let _n0 = graph
                .insert_between("middle", String::from("IL"), String::from("OL"))
                .await?;

            let _n1 = graph
                .insert_between("middle", graph.input.id().into(), String::from("OL"))
                .await?;

            // input -> [n0, n1] -> output
            assert_eq!(graph.len(), 4);

            let path = graph.traverse_from_input_node().await;

            assert_eq!(path.len(), 4);

            Ok(())
        }

        #[tokio::test]
        async fn test_bfs_should_detect_path_between_nodes() -> Result<(), Box<dyn Error>> {
            let mut graph = CyclicGraph::new(
                String::from("IL"),
                "input_data",
                String::from("OL"),
                "output_data",
                0,
                |recent_id, mode| match mode {
                    GeneratorMode::Normal => {
                        format!(
                            "ML_{}",
                            recent_id.fetch_add(1, std::sync::atomic::Ordering::Release)
                        )
                    }
                    GeneratorMode::DryRun => format!(
                        "ML_{}",
                        recent_id.load(std::sync::atomic::Ordering::Acquire)
                    ),
                },
            )
            .await?;

            // input -> [n0, n1 -> n2] -> output

            let n0 = graph
                .insert_between("middle", String::from("IL"), String::from("OL"))
                .await?;

            let n1 = graph
                .insert_between("middle", String::from("IL"), String::from("OL"))
                .await?;

            let n2 = graph
                .insert_between("middle", n1.id().into(), String::from("OL"))
                .await?;

            assert_eq!(graph.len(), 5);

            assert!(graph.bfs(graph.input.clone(), graph.output.clone()).await);
            assert!(!graph.bfs(graph.output.clone(), graph.input.clone()).await);
            assert!(!graph.bfs(n0.clone(), n1.clone()).await);
            assert!(graph.bfs(n1.clone(), n2.clone()).await);

            Ok(())
        }
    }
}
