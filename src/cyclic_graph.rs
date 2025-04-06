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
    nodes: Arc<RwLock<HashMap<I, Arc<Node<I, T>>>>>,
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

        let nodes = Arc::new(RwLock::new(nodes));

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
        self.nodes.write().await.insert(id, new_node.clone());

        for parent_id in parent_ids {
            if let Some(parent) = self.nodes.read().await.get(parent_id) {
                parent.link_child(new_node.clone()).await;
                new_node.link_parent(parent.clone()).await;
            } else {
                return Err(Box::new(CyclicGraphError::NodeNotFoundById(
                    parent_id.clone(),
                )));
            }
        }

        for child_id in child_ids {
            if let Some(child) = self.nodes.read().await.get(child_id) {
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
            .read()
            .await
            .get(&parent_id)
            .ok_or(Box::new(CyclicGraphError::NodeNotFoundById(
                parent_id.clone(),
            )))?
            .clone();
        let child = self
            .nodes
            .read()
            .await
            .get(&child_id)
            .ok_or(Box::new(CyclicGraphError::NodeNotFoundById(
                child_id.clone(),
            )))?
            .clone();

        let id = (self.id_generator)(&self.recent_id, GeneratorMode::Normal);
        let new_node = Arc::new(Node::new(id.clone(), data));
        self.nodes.write().await.insert(id, new_node.clone());

        if parent.has_child(&child_id).await {
            parent.unlink_child(child.clone()).await;
        }

        parent.link_child(new_node.clone()).await;
        child.link_parent(new_node.clone()).await;

        Ok(new_node.clone())
    }

    pub async fn remove(&mut self, id: &I) -> Result<bool, Box<dyn Error>> {
        let r_nodes = self.nodes.read().await;
        let node = r_nodes.get(id).cloned();
        drop(r_nodes);

        match node {
            Some(node) => {
                // prevent removing input or output
                if self.input.id() == id {
                    return Err(Box::new(CyclicGraphError::RemoveInput::<I>));
                } else if self.output.id() == id {
                    return Err(Box::new(CyclicGraphError::RemoveOutput::<I>));
                }

                // prolongation of braking links
                // collect parents
                let mut parents = Vec::new();
                for node_id in node.parent_ids().await.iter() {
                    if let Some(node) = self.nodes.read().await.get(node_id) {
                        parents.push(node.clone());
                    }
                }

                // collect children
                let mut children = Vec::new();
                for node_id in node.child_ids().await.iter() {
                    if let Some(node) = self.nodes.read().await.get(node_id) {
                        children.push(node.clone());
                    }
                }

                // if parents and children are not had links then create links
                // and unlink removing node from parents and children
                for parent in parents {
                    for child in &children {
                        let child = child.clone();
                        if !parent.has_child(child.id()).await {
                            parent.link_child(child.clone()).await;
                        }
                        node.unlink_child(child.clone()).await;
                    }
                    node.unlink_parent(parent.clone()).await;
                }

                // remove node
                Ok(self.nodes.write().await.remove(node.id()).is_some())
            }
            None => Ok(false),
        }
    }

    pub async fn get(&self, id: &I) -> Option<Arc<Node<I, T>>> {
        self.nodes.read().await.get(id).cloned()
    }

    pub async fn len(&self) -> usize {
        self.nodes.read().await.len()
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

        for child_id in children_ids.iter() {
            if let Some(child) = self.nodes.read().await.get(child_id) {
                if visited.write().await.insert(child_id.clone()) {
                    result.write().await.push(child_id.clone());
                    self.dfs(child.clone(), visited.clone(), result.clone())
                        .await;
                }
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
            for id in ids.iter() {
                if let Some(child) = self.nodes.read().await.get(id) {
                    if visited.insert(id.clone()) {
                        queue.push(child.clone());
                    }
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
            assert_eq!(graph.len().await, 2);

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

            assert_eq!(graph.len().await, 4);
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
            assert_eq!(graph.len().await, 4);

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
            assert_eq!(graph.len().await, 4);

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
            assert_eq!(graph.len().await, 4);

            let path = graph.traverse_from_input_node().await;

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
            assert_eq!(graph.len().await, 4);

            let path = graph.traverse_from_input_node().await;

            assert_eq!(path.len(), 4);

            Ok(())
        }

        #[tokio::test]
        async fn test_remove_should_delete_specified_node_and_prolongate_links()
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

            // Graph state before removing n4 node
            //               input
            //               /    \
            //              /      \
            //             n2       n3
            //            /  \     /  \
            //           /    \   /    \
            //          |      n4       |
            //           \  __/ | \__  /
            //            n5    |    n6
            //             \    n7   /
            //              \   |   /
            //               \  |  /
            //               output
            //
            // Graph state after removing n4 node
            //               input
            //               /    \
            //              /      \
            //             n2--\ /--n3
            //            /  \  X   / \
            //           /    \/ \/    \
            //          n5____/n7 \____n6
            //           \      |      /
            //            \     |     /
            //             \    |    /
            //              \   |   /
            //               \  |  /
            //               output

            let n2 = graph.insert_between("middle2", 0, 1).await?;
            let n3 = graph.insert_between("middle3", 0, 1).await?;
            let n4 = graph.insert_between("middle4", n2.id().clone(), 1).await?;
            n4.link_parent(n3.clone()).await;
            let n5 = graph.insert_between("middle5", n2.id().clone(), 1).await?;
            n5.link_parent(n4.clone()).await;
            let n6 = graph.insert_between("middle6", n3.id().clone(), 1).await?;
            n6.link_parent(n4.clone()).await;
            let n7 = graph.insert_between("middle7", n4.id().clone(), 1).await?;

            assert_eq!(graph.input.child_ids().await.len(), 2);
            assert!(graph.input.has_child(&2).await);
            assert!(graph.input.has_child(&3).await);

            assert_eq!(n2.parent_ids().await.len(), 1);
            assert!(n2.has_parent(&0).await);
            assert_eq!(n2.child_ids().await.len(), 2);
            assert!(n2.has_child(&4).await);
            assert!(n2.has_child(&5).await);

            assert_eq!(n3.parent_ids().await.len(), 1);
            assert!(n3.has_parent(&0).await);
            assert_eq!(n3.child_ids().await.len(), 2);
            assert!(n3.has_child(&4).await);
            assert!(n3.has_child(&6).await);

            assert_eq!(n4.parent_ids().await.len(), 2);
            assert!(n4.has_parent(&2).await);
            assert!(n4.has_parent(&3).await);
            assert_eq!(n4.child_ids().await.len(), 3);
            assert!(n4.has_child(&5).await);
            assert!(n4.has_child(&6).await);
            assert!(n4.has_child(&7).await);

            assert_eq!(n5.parent_ids().await.len(), 2);
            assert!(n5.has_parent(&2).await);
            assert!(n5.has_parent(&4).await);
            assert_eq!(n5.child_ids().await.len(), 1);
            assert!(n5.has_child(&1).await);

            assert_eq!(n6.parent_ids().await.len(), 2);
            assert!(n6.has_parent(&3).await);
            assert!(n6.has_parent(&4).await);
            assert_eq!(n6.child_ids().await.len(), 1);
            assert!(n6.has_child(&1).await);

            assert_eq!(n7.parent_ids().await.len(), 1);
            assert!(n7.has_parent(&4).await);
            assert_eq!(n7.child_ids().await.len(), 1);
            assert!(n7.has_child(&1).await);

            assert_eq!(graph.output.parent_ids().await.len(), 3);
            assert!(graph.output.has_parent(&5).await);
            assert!(graph.output.has_parent(&6).await);
            assert!(graph.output.has_parent(&7).await);

            assert_eq!(graph.len().await, 8);

            let res = graph.remove(&4).await;
            assert!(res.is_ok());
            assert!(res.unwrap());

            assert_eq!(graph.len().await, 7);

            assert_eq!(graph.input.child_ids().await.len(), 2);
            assert!(graph.input.has_child(&2).await);
            assert!(graph.input.has_child(&3).await);

            assert_eq!(n2.parent_ids().await.len(), 1);
            assert!(n2.has_parent(&0).await);
            assert_eq!(n2.child_ids().await.len(), 3);
            assert!(n2.has_child(&5).await);
            assert!(n2.has_child(&6).await);
            assert!(n2.has_child(&7).await);

            assert_eq!(n3.parent_ids().await.len(), 1);
            assert!(n3.has_parent(&0).await);
            assert_eq!(n3.child_ids().await.len(), 3);
            assert!(n3.has_child(&5).await);
            assert!(n3.has_child(&6).await);
            assert!(n3.has_child(&7).await);

            assert_eq!(n5.parent_ids().await.len(), 2);
            assert!(n5.has_parent(&2).await);
            assert!(n5.has_parent(&3).await);
            assert_eq!(n5.child_ids().await.len(), 1);
            assert!(n5.has_child(&1).await);

            assert_eq!(n6.parent_ids().await.len(), 2);
            assert!(n6.has_parent(&2).await);
            assert!(n6.has_parent(&3).await);
            assert_eq!(n6.child_ids().await.len(), 1);
            assert!(n6.has_child(&1).await);

            assert_eq!(n7.parent_ids().await.len(), 2);
            assert!(n7.has_parent(&2).await);
            assert!(n7.has_parent(&3).await);
            assert_eq!(n7.child_ids().await.len(), 1);
            assert!(n7.has_child(&1).await);

            assert_eq!(graph.output.parent_ids().await.len(), 3);
            assert!(graph.output.has_parent(&5).await);
            assert!(graph.output.has_parent(&6).await);
            assert!(graph.output.has_parent(&7).await);

            // try to remove again. remove should remove Ok(false)
            let res = graph.remove(&4).await;
            assert!(res.is_ok());
            assert!(!res.unwrap());

            Ok(())
        }

        #[tokio::test]
        async fn test_get_node_by_node_id() -> Result<(), Box<dyn Error>> {
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

            // input -> n2 -> output
            assert_eq!(graph.len().await, 3);

            let node_opt = graph.get(&2).await;
            assert!(node_opt.is_some());
            let node = node_opt.unwrap();
            assert!(node.data().read().await.contains("middle2"));
            assert_eq!(node.id(), &2);

            // try get non-existing node
            let node_opt = graph.get(&10);
            assert!(node_opt.await.is_none());

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
            assert_eq!(graph.len().await, 2);

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

            assert_eq!(graph.len().await, 4);
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
            assert_eq!(graph.len().await, 4);

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
            assert_eq!(graph.len().await, 4);

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
            assert_eq!(graph.len().await, 4);

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
            assert_eq!(graph.len().await, 4);

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

            assert_eq!(graph.len().await, 5);

            assert!(graph.bfs(graph.input.clone(), graph.output.clone()).await);
            assert!(!graph.bfs(graph.output.clone(), graph.input.clone()).await);
            assert!(!graph.bfs(n0.clone(), n1.clone()).await);
            assert!(graph.bfs(n1.clone(), n2.clone()).await);

            Ok(())
        }
    }
}
