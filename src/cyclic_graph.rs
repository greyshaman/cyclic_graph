use std::{
    collections::{HashMap, HashSet, hash_map::Entry},
    error::Error,
    fmt::{Debug, Display},
    hash::Hash,
    sync::{Arc, atomic::AtomicUsize},
};

use async_recursion::async_recursion;
use tokio::sync::RwLock;

use crate::{Content, Error as CGError, GeneratorMode, IdGenerator, node::Node};

pub type NodesMap<I, D, S> = Arc<RwLock<HashMap<I, Arc<Node<I, D, S>>>>>;

/// A graph with one input node, many intermediate nodes, and one output node.
/// The graph keeps track of the uniqueness of node identifiers,
/// allowing you to add new nodes and create connections to existing nodes within the graph.
/// In the initial state, the graph has only input and output nodes that are connected.
/// These nodes cannot be removed or moved.
/// The input node is always the starting point for other nodes in the graph,
/// and the output is always an ending point for other nodes.
/// I - the nodes identifier type
/// D - the type of nodes payload data
/// S - the type of signal translated between inner elements of node content
/// G - identifier generator function
pub struct CyclicGraph<I, D: 'static, S = (), G = fn(&AtomicUsize, GeneratorMode) -> I>
where
    I: Clone + Eq + Hash + Debug + Display + Sync + Send + 'static,
    D: Send + Sync + 'static + Clone + Debug,
    G: Fn(&AtomicUsize, GeneratorMode) -> I,
    S: 'static + Send + Sync + Debug,
{
    /// The input node
    input: Arc<Node<I, D, S>>,

    /// The output node
    output: Arc<Node<I, D, S>>,

    /// The map of nodes
    nodes: NodesMap<I, D, S>,

    /// The id generator function
    id_generator: G,

    /// Helper to id generator function
    recent_id: AtomicUsize,
}

impl<I, D, S> CyclicGraph<I, D, S, fn(&AtomicUsize, GeneratorMode) -> I>
where
    I: Clone + Eq + Hash + Debug + Display + Send + Sync + 'static,
    D: Send + Sync + 'static + Clone + Debug,
    S: 'static + Send + Sync + Debug,
    (): IdGenerator<I>,
{
    /// Creates new graph with default id generator implementation and simple content.
    /// Parameters are using for initial graph configuration.
    /// Using `input_id` - to specify input node identifier,
    /// `input_data` - to specify input node content value,
    /// `output_id` - to specify output identifier which should differ from input_id,
    /// `output_data` - to specify output node content value,
    /// `start_id_idx` - id usize number from which start counter to generate unique
    /// middle nodes. Generated id should be differ from input_id and output_id.
    pub fn new_default(
        input_id: I,
        input_data: Content<I, D, S>,
        output_id: I,
        output_data: Content<I, D, S>,
        start_id_idx: usize,
    ) -> Result<Self, Box<dyn Error>> {
        Self::new(
            input_id,
            input_data,
            output_id,
            output_data,
            start_id_idx,
            |recent_id, mode| <() as IdGenerator<I>>::generate_id(recent_id, mode),
        )
    }
}

impl<I, D, S, G> CyclicGraph<I, D, S, G>
where
    I: Clone + Eq + Hash + Debug + Display + Sync + Send + 'static,
    D: Send + Sync + 'static + Clone + Debug,
    S: 'static + Send + Sync + Debug,
    G: Fn(&AtomicUsize, GeneratorMode) -> I + Sync + Send + 'static,
{
    /// Creates new graph with input and output nodes.
    /// Parameters are using for initial graph configuration.
    /// Using `input_id` - to specify input node identifier,
    /// `input_data` - to specify input node content data,
    /// `output_id` - to specify output identifier which should differ from input_id,
    /// `output_data` - to specify output node content data,
    /// `start_id_idx` - is usize number from which start counter to generate unique
    ///  middle nodes. Generated id should be differ from input_id and output_id.
    /// The `id_generator` - is using to set generator function executed when new node
    /// would append or insert into graph.
    ///
    /// # Example for graph with usize id nodes:
    ///
    /// ```rust
    /// use cyclic_graph::{CyclicGraph, GeneratorMode, Content, Error as CGError};
    /// use std::sync::atomic::Ordering;
    /// use std::error::Error;
    /// use std::sync::Arc;
    /// use tokio::sync::RwLock;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn Error>> {
    ///     let graph = CyclicGraph::new(
    ///         0_usize, // input_id
    ///         Content::<usize, String>::new_simple("input".to_string()), // payload data for input node
    ///         1, // output_id
    ///         Content::new_simple("output".to_string()), // payload data for output node
    ///         2, // start_id_idx - from this number generator will be generate id for new nodes
    ///         |recent_id, mode| match mode {
    ///             GeneratorMode::Normal => {
    ///                 recent_id.fetch_add(1, Ordering::Relaxed)
    ///             }
    ///             GeneratorMode::DryRun => recent_id.load(Ordering::Relaxed),
    ///         }
    ///     )?;
    ///
    ///     assert_eq!(graph.len().await, 2);
    ///     Ok(())
    /// }
    /// ```
    ///
    /// # Example for graph with String id node:
    ///
    /// ```rust
    /// use cyclic_graph::{CyclicGraph, GeneratorMode, Content, Error as CGError};
    /// use std::{sync::atomic::Ordering, error::Error};
    /// use std::sync::Arc;
    /// use tokio::sync::RwLock;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn Error>> {
    ///     let graph = CyclicGraph::new(
    ///         String::from("IL"), // input_id
    ///         Content::<String, String>::new_simple("input".to_string()), // input_data
    ///         String::from("OL"), // output_id
    ///         Content::new_simple("output".to_string()), // output_data
    ///         0, // start_id_idx
    ///         |recent_id, mode| match mode {
    ///             GeneratorMode::Normal => {
    ///                 format!(
    ///                     "ML_{}",
    ///                     recent_id.fetch_add(1, Ordering::Release),
    ///                 )
    ///             }
    ///             GeneratorMode::DryRun => {
    ///                 format!(
    ///                     "ML_{}",
    ///                     recent_id.load(Ordering::Acquire),
    ///                 )
    ///             }
    ///         }
    ///     )?;
    ///
    ///     assert_eq!(graph.len().await, 2);
    ///
    ///     Ok(())
    /// }
    /// ```
    pub fn new(
        input_id: I,
        input_content: Content<I, D, S>,
        output_id: I,
        output_content: Content<I, D, S>,
        start_id_idx: usize,
        id_generator: G,
    ) -> Result<Self, Box<dyn Error>> {
        if input_id == output_id {
            return Err(Box::new(CGError::NonUniqueId(output_id.clone())));
        }

        let input = Arc::new(Node::new(input_id, input_content));
        let output = Arc::new(Node::new(output_id, output_content));

        let mut nodes = HashMap::new();
        nodes.insert(input.id().clone(), input.clone());
        nodes.insert(output.id().clone(), output.clone());

        let nodes = Arc::new(RwLock::new(nodes));

        input.try_link_child(output.clone())?;

        let recent_id = AtomicUsize::new(start_id_idx);
        let try_id = id_generator(&recent_id, GeneratorMode::DryRun);

        if input.id() == &try_id || output.id() == &try_id {
            return Err(Box::new(CGError::NonUniqueId(try_id.clone())));
        }

        Ok(Self {
            input,
            output,
            nodes,
            id_generator,
            recent_id,
        })
    }

    /// Appends node to graph with create links to specified
    /// parents `parent_ids` and children `child_ids` by ids.
    ///
    /// The payload node data sets by `data` parameter
    ///
    /// Before:
    ///
    /// #       +-------------+
    /// #       | Parent Node |
    /// #       +-------------+
    /// #               |
    /// #               |
    /// #               |
    /// #               |
    /// #               V
    /// #       +------------+
    /// #       | Child Node |
    /// #       +------------+
    ///
    /// After append:
    ///
    /// #       +-------------+
    /// #       | Parent Node |-----+
    /// #       +-------------+     |
    /// #               |           V
    /// #               |      +----------+
    /// #               |      | New Node |
    /// #               |      +----------+
    /// #               |           |
    /// #               V           |
    /// #       +------------+      |
    /// #       | Child Node |<-----+
    /// #       +------------+
    ///
    /// # Example
    ///
    /// ```rust
    /// use cyclic_graph::{CyclicGraph, GeneratorMode, Content};
    /// use std::{sync::atomic::Ordering, error::Error};
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn Error>> {
    ///     let mut graph = CyclicGraph::new(
    ///         0,
    ///         Content::<usize, String>::new_simple("start".to_string()),
    ///         1,
    ///         Content::new_simple("end".to_string()),
    ///         2,
    ///         |recent_id, mode| match mode {
    ///             GeneratorMode::Normal => recent_id.fetch_add(1, Ordering::Relaxed),
    ///             GeneratorMode::DryRun => recent_id.load(Ordering::Relaxed),
    ///         },
    ///     )?;
    ///
    ///     let n = graph.append_node(Content::new_simple("hidden".to_string()), &[0], &[1]).await?;
    ///
    ///     assert_eq!(graph.len().await, 3);
    ///     Ok(())
    /// }
    /// ```
    pub async fn append_node(
        &self,
        content: Content<I, D, S>,
        parent_ids: &[I],
        child_ids: &[I],
    ) -> Result<Arc<Node<I, D, S>>, Box<dyn Error>> {
        // Checking boundary conditions without blocking
        if child_ids.iter().any(|id| id == self.input.id()) {
            return Err(Box::new(CGError::InsertBeforeInput::<I>));
        }

        if parent_ids.iter().any(|id| id == self.output.id()) {
            return Err(Box::new(CGError::InsertAfterOutput::<I>));
        }

        // Atomic ID generation and node creation
        let id = (self.id_generator)(&self.recent_id, GeneratorMode::Normal);
        let new_node = Arc::new(Node::new(id.clone(), content));

        // Single lock for insertion
        {
            let mut nodes = self.nodes.write().await;
            match nodes.entry(id.clone()) {
                Entry::Vacant(entry) => entry.insert(new_node.clone()),
                Entry::Occupied(_) => return Err(Box::new(CGError::NonUniqueId(id))),
            };
        }

        // Parallel processing of links
        let mut parent_links: Vec<tokio::task::JoinHandle<Result<bool, CGError<I>>>> = Vec::new();
        let mut child_links = Vec::new();

        for parent_id in parent_ids {
            let nodes = self.nodes.clone();
            let new_node = new_node.clone();
            let parent_id = parent_id.clone();

            parent_links.push(tokio::spawn(async move {
                let nodes_binding = nodes.read().await;
                let parent = nodes_binding
                    .get(&parent_id)
                    .ok_or(CGError::NodeNotFoundById(parent_id))?;

                parent.link_child(new_node.clone()).await?;
                new_node.link_parent(parent.clone()).await
            }));
        }

        for child_id in child_ids {
            let nodes = self.nodes.clone();
            let new_node = new_node.clone();
            let child_id = child_id.clone();

            child_links.push(tokio::spawn(async move {
                let nodes_binding = nodes.read().await;
                let child = nodes_binding
                    .get(&child_id)
                    .ok_or(CGError::NodeNotFoundById(child_id))?;

                child.link_parent(new_node.clone()).await?;
                new_node.link_child(child.clone()).await
            }));
        }

        // Parallel execution of all binding operations
        let (parent_results, child_results) = tokio::join!(
            futures::future::try_join_all(parent_links),
            futures::future::try_join_all(child_links)
        );

        parent_results.and(child_results)?;

        Ok(new_node)
    }

    /// Inserts node to graph with inset into links between
    /// parent `parent_id` and children `child_id` by ids.
    ///
    /// The payload node data sets by `data` parameter
    ///
    /// Before:
    ///
    /// #        +-------------+
    /// #        | Parent Node |
    /// #        +-------------+
    /// #                |
    /// #                |
    /// #                |
    /// #                |
    /// #                V
    /// #        +------------+
    /// #        | Child Node |
    /// #        +------------+
    ///
    /// After insert_between:
    ///
    /// #        +-------------+
    /// #        | Parent Node |
    /// #        +-------------+
    /// #                |
    /// #                V
    /// #           +----------+
    /// #           | New Node |
    /// #           +----------+
    /// #                |
    /// #                |
    /// #                |
    /// #                V
    /// #        +------------+
    /// #        | Child Node |
    /// #        +------------+
    ///
    ///
    /// # Example
    ///
    /// ```rust
    /// use cyclic_graph::{CyclicGraph, GeneratorMode, Content, Error as CGError};
    /// use std::{sync::atomic::Ordering, error::Error};
    /// use std::sync::Arc;
    /// use tokio::sync::RwLock;
    ///
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn Error>> {
    ///     let mut graph = CyclicGraph::new(
    ///         0,
    ///         Content::<usize, String>::new_simple("start".to_string()),
    ///         1,
    ///         Content::new_simple("end".to_string()),
    ///         2,
    ///         |recent_id, mode| match mode {
    ///             GeneratorMode::Normal => recent_id.fetch_add(1, Ordering::Relaxed),
    ///             GeneratorMode::DryRun => recent_id.load(Ordering::Relaxed),
    ///         },
    ///     )?;
    ///
    ///     let n = graph.insert_between(
    ///         Content::new_simple("hidden".to_string()),
    ///         0,
    ///         1
    ///     ).await?;
    ///
    ///     assert_eq!(graph.len().await, 3);
    ///     Ok(())
    /// }
    /// ```
    pub async fn insert_between(
        &self,
        content: Content<I, D, S>,
        parent_id: I,
        child_id: I,
    ) -> Result<Arc<Node<I, D, S>>, Box<dyn Error>> {
        if &child_id == self.input.id() {
            return Err(Box::new(CGError::InsertBeforeInput::<I>));
        } else if &parent_id == self.output.id() {
            return Err(Box::new(CGError::InsertAfterOutput::<I>));
        }

        let parent = self
            .get(&parent_id)
            .await
            .ok_or(Box::new(CGError::NodeNotFoundById(parent_id.clone())))?
            .clone();
        let child = self
            .get(&child_id)
            .await
            .ok_or(Box::new(CGError::NodeNotFoundById(child_id.clone())))?
            .clone();

        let id = (self.id_generator)(&self.recent_id, GeneratorMode::Normal);
        let new_node = Arc::new(Node::new(id.clone(), content));
        self.nodes.write().await.insert(id, new_node.clone());

        if parent.has_child(&child_id).await {
            parent.unlink_child(child.clone()).await?;
        }

        parent.link_child(new_node.clone()).await?;
        child.link_parent(new_node.clone()).await?;

        Ok(new_node.clone())
    }

    /// Removes node to graph with restoring links between
    /// parents and children specified by `id` reference.
    ///
    /// Returns boolean result if removing was success
    /// or error if trying to remove input or output nodes
    ///
    /// # Example
    ///
    /// ```rust
    /// use cyclic_graph::{CyclicGraph, GeneratorMode, Content, Error as CGError};
    /// use std::sync::atomic::Ordering;
    /// use std::error::Error;
    /// use std::sync::Arc;
    /// use tokio::sync::RwLock;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn Error>> {
    ///     let mut graph = CyclicGraph::new(
    ///         0,
    ///         Content::<usize, String>::new_simple("start".to_string()),
    ///         1,
    ///         Content::new_simple("end".to_string()),
    ///         2,
    ///         |recent_id, mode| match mode {
    ///             GeneratorMode::Normal => recent_id.fetch_add(1, Ordering::Relaxed),
    ///             GeneratorMode::DryRun => recent_id.load(Ordering::Relaxed),
    ///         },
    ///     )?;
    ///
    ///     let n = graph.insert_between(
    ///         Content::new_simple("hidden".to_string()),
    ///         0,
    ///         1
    ///     ).await?;
    ///
    ///     assert_eq!(graph.len().await, 3);
    ///
    ///     assert!(graph.remove(&2).await?);
    ///     assert_eq!(graph.len().await, 2);
    ///
    ///     Ok(())
    /// }
    /// ```
    pub async fn remove(&self, id: &I) -> Result<bool, Box<dyn Error>> {
        // Quick checks without blocking
        if self.input.id() == id {
            return Err(Box::new(CGError::RemoveInput::<I>));
        }

        if self.output.id() == id {
            return Err(Box::new(CGError::RemoveOutput::<I>));
        }

        // Atomic node extraction
        let node = {
            let mut nodes = self.nodes.write().await;
            match nodes.remove(id) {
                Some(node) => node,
                None => return Ok(false),
            }
        };

        // Parallel collection of links
        let (parent_ids, child_ids) = tokio::join!(node.parent_ids(), node.child_ids());

        // Prepare data for parallel operation
        let nodes = self.nodes.clone();
        let node_ref = node.clone();

        // Concurrent breaking links
        let break_parent_links = async {
            let mut results = Vec::new();
            for parent_id in &parent_ids {
                let parent = match nodes.read().await.get(parent_id) {
                    Some(p) => p.clone(),
                    None => continue,
                };
                results.push(parent.unlink_child(node_ref.clone()).await);
            }
            results
        };

        let break_child_links = async {
            let mut results = Vec::new();
            for child_id in &child_ids {
                let child = match nodes.read().await.get(child_id) {
                    Some(c) => c.clone(),
                    None => continue,
                };
                results.push(child.unlink_parent(node_ref.clone()).await);
            }
            results
        };

        // parallel execution
        let (parent_results, child_results) = tokio::join!(break_parent_links, break_child_links);

        // restore links between remaining nodes
        self.reconnect_nodes(&parent_ids, &child_ids).await?;

        // check results
        let all_ok = parent_results
            .into_iter()
            .chain(child_results)
            .all(|r| r.unwrap_or(false));
        Ok(all_ok)
    }

    async fn reconnect_nodes(
        &self,
        parent_ids: &[I],
        child_ids: &[I],
    ) -> Result<(), Box<dyn Error>> {
        let nodes = self.nodes.read().await;

        let parents = parent_ids
            .iter()
            .filter_map(|id| nodes.get(id))
            .cloned()
            .collect::<Vec<_>>();

        let children = child_ids
            .iter()
            .filter_map(|id| nodes.get(id))
            .cloned()
            .collect::<Vec<_>>();

        for parent in &parents {
            for child in &children {
                if !parent.has_child(child.id()).await {
                    parent.link_child(child.clone()).await?;
                }
            }
        }

        Ok(())
    }

    /// Returns the option of wrapped node reference from graph
    /// specified by `id` reference
    ///
    /// # Example
    ///
    /// ```rust
    /// use cyclic_graph::{CyclicGraph, GeneratorMode, Content, Error as CGError};
    /// use std::sync::atomic::Ordering;
    /// use std::error::Error;
    /// use std::sync::Arc;
    /// use tokio::sync::RwLock;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn Error>> {
    ///     let mut graph = CyclicGraph::new(
    ///         0,
    ///         Content::<usize, String>::new_simple("start".to_string()),
    ///         1,
    ///         Content::new_simple("end".to_string()),
    ///         2,
    ///         |recent_id, mode| match mode {
    ///             GeneratorMode::Normal => recent_id.fetch_add(1, Ordering::Relaxed),
    ///             GeneratorMode::DryRun => recent_id.load(Ordering::Relaxed),
    ///         },
    ///     )?;
    ///
    ///     let n = graph.insert_between(
    ///         Content::new_simple("hidden".to_string()),
    ///         0,
    ///         1
    ///     ).await?;
    ///
    ///     assert_eq!(graph.get(&0).await.unwrap().id(), &0);
    ///     assert_eq!(graph.get(&2).await.unwrap().id(), &2);
    ///     assert!(graph.get(&3).await.is_none());
    ///     Ok(())
    /// }
    /// ```
    pub async fn get(&self, id: &I) -> Option<Arc<Node<I, D, S>>> {
        self.nodes.read().await.get(id).cloned()
    }

    /// Returns the reference to input node
    pub fn input(&self) -> Arc<Node<I, D, S>> {
        Arc::clone(&self.input)
    }

    /// Returns the reference to output node
    pub fn output(&self) -> Arc<Node<I, D, S>> {
        Arc::clone(&self.output)
    }

    /// Returns number of nodes in the graph.
    #[allow(clippy::len_without_is_empty)]
    pub async fn len(&self) -> usize {
        self.nodes.read().await.len()
    }

    /// Returns the path of the graph.
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
        node: Arc<Node<I, D, S>>,
        visited: Arc<RwLock<HashSet<I>>>,
        result: Arc<RwLock<Vec<I>>>,
    ) {
        let children_ids = node.child_ids().await;

        for child_id_ref in children_ids.iter() {
            let child_id = child_id_ref.clone();

            if let Some(child) = self.nodes.read().await.get(child_id_ref) {
                let mut w_visited = visited.write().await;
                if w_visited.insert(child_id.clone()) {
                    drop(w_visited);

                    let mut w_result = result.write().await;
                    w_result.push(child_id.clone());
                    drop(w_result);

                    self.dfs(child.clone(), visited.clone(), result.clone())
                        .await;
                }
            }
        }
    }

    pub async fn bfs(&self, from_node: Arc<Node<I, D, S>>, goal_node: Arc<Node<I, D, S>>) -> bool {
        let mut visited = HashSet::<I>::new();
        let mut queue = Vec::<Arc<Node<I, D, S>>>::new();

        queue.push(from_node.clone());
        while let Some(node) = queue.pop() {
            if node.id() == goal_node.id() {
                return true;
            }
            visited.insert(node.id().clone());

            let ids = node.child_ids().await;
            for id in ids.iter() {
                if let Some(child) = self.nodes.read().await.get(id)
                    && visited.insert(id.clone())
                {
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
        async fn test_new_default_can_create_cyclic_graph() -> Result<(), Box<dyn Error>> {
            let input_content = Content::<usize, String>::new_simple("input_data".to_string());
            let output_content = Content::new_simple("output_data".to_string());
            let graph = CyclicGraph::new_default(0, input_content, 1, output_content, 2)?;

            assert_eq!(graph.input.id(), &0);
            assert_eq!(graph.output.id(), &1);
            assert_eq!(graph.len().await, 2);

            assert!(graph.input.has_child(&1).await);
            assert!(graph.output.has_parent(&0).await);

            Ok(())
        }

        #[tokio::test]
        async fn test_new_default_should_return_error_when_terminal_nodes_has_same_ids() {
            let input_content = Content::<usize, String>::new_simple("input_data".to_string());
            let output_content = Content::new_simple("output_data".to_string());
            let result = CyclicGraph::new_default(0, input_content, 0, output_content, 2);

            assert!(result.is_err());
        }

        #[tokio::test]
        async fn test_new_default_should_return_error_when_start_id_idx_same_of_input_node() {
            let input_content = Content::<usize, String>::new_simple("input_data".to_string());
            let output_content = Content::new_simple("output_data".to_string());
            let result = CyclicGraph::new_default(0, input_content, 1, output_content, 0);

            assert!(result.is_err());
        }

        #[tokio::test]
        async fn test_new_default_should_return_error_when_start_id_idx_same_of_output_node() {
            let input_content = Content::<usize, String>::new_simple("input_data".to_string());
            let output_content = Content::new_simple("output_data".to_string());
            let result = CyclicGraph::new_default(0, input_content, 1, output_content, 1);

            assert!(result.is_err());
        }

        #[tokio::test]
        async fn test_append_node_can_add_new_node_to_empty_graph() -> Result<(), Box<dyn Error>> {
            let input_content = Content::<usize, String>::new_simple("input_data".to_string());
            let output_content = Content::new_simple("output_data".to_string());
            let graph = CyclicGraph::new_default(0, input_content, 1, output_content, 2)?;

            let hidden2_content = Content::new_simple("hidden2".to_string());
            let hidden3_content = Content::new_simple("hidden3".to_string());
            let n2 = graph.append_node(hidden2_content, &[0], &[1]).await?;
            let n3 = graph.append_node(hidden3_content, &[0], &[1]).await?;

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
            let input_content = Content::<usize, String>::new_simple("input_data".to_string());
            let output_content = Content::new_simple("output_data".to_string());
            let graph = CyclicGraph::new_default(0, input_content, 1, output_content, 2).unwrap();

            let hidden_content = Content::new_simple("hidden".to_string());
            let result = graph.append_node(hidden_content, &[0], &[0]).await;

            assert!(result.is_err());
        }

        #[tokio::test]
        async fn test_append_node_should_return_error_when_output_id_in_parent_param() {
            let input_content = Content::<usize, String>::new_simple("input_data".to_string());
            let output_content = Content::new_simple("output_data".to_string());
            let graph = CyclicGraph::new_default(0, input_content, 1, output_content, 2).unwrap();

            let hidden_content = Content::new_simple("hidden".to_string());
            let result = graph.append_node(hidden_content, &[1], &[1]).await;

            assert!(result.is_err());
        }

        #[tokio::test]
        async fn test_serial_insert_between_create_and_inset_new_nodes_between_specified_nodes()
        -> Result<(), Box<dyn Error>> {
            let input_content = Content::<usize, String>::new_simple("input_data".to_string());
            let output_content = Content::new_simple("output_data".to_string());
            let graph = CyclicGraph::new_default(0, input_content, 1, output_content, 2).unwrap();

            let hidden_content = Content::new_simple("middle".to_string());
            let result = graph.insert_between(hidden_content, 0, 1).await;
            assert!(result.is_ok());
            let n2 = result.unwrap();

            let hidden_content2 = Content::new_simple("middle2".to_string());
            let n3 = graph.insert_between(hidden_content2, 2, 1).await?;

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
            let input_content = Content::<usize, String>::new_simple("input_data".to_string());
            let output_content = Content::new_simple("output_data".to_string());
            let graph = CyclicGraph::new_default(0, input_content, 1, output_content, 2)?;

            let hidden_content2 = Content::new_simple("middle2".to_string());
            let result = graph.insert_between(hidden_content2, 0, 1).await;
            assert!(result.is_ok());
            let n2 = result.unwrap();

            let hidden_content3 = Content::new_simple("middle3".to_string());
            let n3 = graph.insert_between(hidden_content3, 0, 1).await?;

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
            let input_content = Content::<usize, String>::new_simple("input_data".to_string());
            let output_content = Content::new_simple("output_data".to_string());
            let graph = CyclicGraph::new_default(0, input_content, 1, output_content, 2)?;

            let hidden_content = Content::new_simple("middle2".to_string());
            let n2 = graph.insert_between(hidden_content, 0, 1).await?;

            let hidden_content3 = Content::new_simple("middle3".to_string());
            let _n3 = graph
                .insert_between(hidden_content3, n2.id().clone(), 1)
                .await?;

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
            let input_content = Content::<usize, String>::new_simple("input_data".to_string());
            let output_content = Content::new_simple("output_data".to_string());
            let graph = CyclicGraph::new_default(0, input_content, 1, output_content, 2)?;

            let hidden_content2 = Content::new_simple("middle2".to_string());
            let _n2 = graph.insert_between(hidden_content2, 0, 1).await?;

            let hidden_content3 = Content::new_simple("middle3".to_string());
            let _n3 = graph.insert_between(hidden_content3, 0, 1).await?;

            // input -> [n2, n3] -> output
            assert_eq!(graph.len().await, 4);

            let path = graph.traverse_from_input_node().await;

            assert_eq!(path.len(), 4);

            Ok(())
        }

        #[tokio::test]
        async fn test_remove_should_delete_specified_node_and_prolongate_links()
        -> Result<(), Box<dyn Error>> {
            let input_content = Content::<usize, String>::new_simple("input_data".to_string());
            let output_content = Content::new_simple("output_data".to_string());
            let graph = CyclicGraph::new_default(0, input_content, 1, output_content, 2)?;

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

            let n2 = graph
                .insert_between(Content::new_simple("middle2".to_string()), 0, 1)
                .await?;
            let n3 = graph
                .insert_between(Content::new_simple("middle3".to_string()), 0, 1)
                .await?;
            let n4 = graph
                .insert_between(
                    Content::new_simple("middle4".to_string()),
                    n2.id().clone(),
                    1,
                )
                .await?;
            n4.link_parent(n3.clone()).await?;
            let n5 = graph
                .insert_between(
                    Content::new_simple("middle5".to_string()),
                    n2.id().clone(),
                    1,
                )
                .await?;
            n5.link_parent(n4.clone()).await?;
            let n6 = graph
                .insert_between(
                    Content::new_simple("middle6".to_string()),
                    n3.id().clone(),
                    1,
                )
                .await?;
            n6.link_parent(n4.clone()).await?;
            let n7 = graph
                .insert_between(
                    Content::new_simple("middle6".to_string()),
                    n4.id().clone(),
                    1,
                )
                .await?;

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
            let input_content = Content::<usize, String>::new_simple("input_data".to_string());
            let output_content = Content::new_simple("output_data".to_string());
            let graph = CyclicGraph::new_default(0, input_content, 1, output_content, 2)?;

            let _n2 = graph
                .insert_between(Content::new_simple("middle2".to_string()), 0, 1)
                .await?;

            // input -> n2 -> output
            assert_eq!(graph.len().await, 3);

            let node_opt = graph.get(&2).await;
            assert!(node_opt.is_some());
            let node = node_opt.unwrap();
            assert!(node.value().await.unwrap().contains("middle2"));
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
        async fn test_new_default_can_create_cyclic_graph() -> Result<(), Box<dyn Error>> {
            let graph = CyclicGraph::new_default(
                String::from("IL"),
                Content::<String, String>::new_simple("input_data".to_string()),
                String::from("OL"),
                Content::new_simple("output_data".to_string()),
                0,
            )?;

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
        async fn test_new_default_should_return_error_when_terminal_nodes_has_same_ids() {
            let result = CyclicGraph::new_default(
                String::from("IL"),
                Content::<String, String>::new_simple("input_data".to_string()),
                String::from("IL"),
                Content::new_simple("output_data".to_string()),
                0,
            );

            assert!(result.is_err());
        }

        #[tokio::test]
        async fn test_append_node_should_add_new_nodes_with_correct_ids()
        -> Result<(), Box<dyn Error>> {
            let graph = CyclicGraph::new_default(
                String::from("IL"),
                Content::<String, String>::new_simple("input_data".to_string()),
                String::from("OL"),
                Content::new_simple("output_data".to_string()),
                0,
            )?;

            let new_node = graph
                .append_node(
                    Content::new_simple("hidden1".to_string()),
                    &["IL".to_string()],
                    &["OL".to_string()],
                )
                .await?;

            let new_node2 = graph
                .append_node(
                    Content::new_simple("hidden2".to_string()),
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
            let graph = CyclicGraph::new_default(
                String::from("IL"),
                Content::<String, String>::new_simple("input_data".to_string()),
                String::from("OL"),
                Content::new_simple("output_data".to_string()),
                0,
            )
            .unwrap();
            let result = graph
                .append_node(
                    Content::new_simple("hidden".to_string()),
                    &[String::from("IL")],
                    &[String::from("IL")],
                )
                .await;

            assert!(result.is_err());
        }

        #[tokio::test]
        async fn test_append_node_should_return_error_when_output_id_in_parent_param() {
            let graph = CyclicGraph::new_default(
                String::from("IL"),
                Content::<String, String>::new_simple("input_data".to_string()),
                String::from("OL"),
                Content::new_simple("output_data".to_string()),
                0,
            )
            .unwrap();
            let result = graph
                .append_node(
                    Content::new_simple("hidden".to_string()),
                    &[String::from("OL")],
                    &[String::from("OL")],
                )
                .await;

            assert!(result.is_err());
        }

        #[tokio::test]
        async fn test_serial_insert_between_create_and_inset_new_nodes_between_specified_nodes()
        -> Result<(), Box<dyn Error>> {
            let graph = CyclicGraph::new_default(
                String::from("IL"),
                Content::<String, String>::new_simple("input_data".to_string()),
                String::from("OL"),
                Content::new_simple("output_data".to_string()),
                0,
            )
            .unwrap();

            let result = graph
                .insert_between(
                    Content::new_simple("middle0".to_string()),
                    String::from("IL"),
                    String::from("OL"),
                )
                .await;
            assert!(result.is_ok());
            let n0 = result.unwrap();

            let n1 = graph
                .insert_between(
                    Content::new_simple("middle1".to_string()),
                    n0.id().into(),
                    String::from("OL"),
                )
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
            let graph = CyclicGraph::new_default(
                String::from("IL"),
                Content::<String, String>::new_simple("input_data".to_string()),
                String::from("OL"),
                Content::new_simple("output".to_string()),
                0,
            )
            .unwrap();

            let result = graph
                .insert_between(
                    Content::new_simple("middle0".to_string()),
                    String::from("IL"),
                    String::from("OL"),
                )
                .await;
            assert!(result.is_ok());
            let n0 = result.unwrap();

            let n1 = graph
                .insert_between(
                    Content::new_simple("middle1".to_string()),
                    graph.input.id().into(),
                    graph.output.id().into(),
                )
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
            let graph = CyclicGraph::new_default(
                String::from("IL"),
                Content::<String, String>::new_simple("input".to_string()),
                String::from("OL"),
                Content::new_simple("output".to_string()),
                0,
            )?;

            let n0 = graph
                .insert_between(
                    Content::new_simple("middle".to_string()),
                    String::from("IL"),
                    String::from("OL"),
                )
                .await?;

            let n1 = graph
                .insert_between(
                    Content::new_simple("middle".to_string()),
                    n0.id().into(),
                    String::from("OL"),
                )
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
            let graph = CyclicGraph::new_default(
                String::from("IL"),
                Content::<String, String>::new_simple("input".to_string()),
                String::from("OL"),
                Content::new_simple("output".to_string()),
                0,
            )?;

            let _n0 = graph
                .insert_between(
                    Content::new_simple("middle".to_string()),
                    String::from("IL"),
                    String::from("OL"),
                )
                .await?;

            let _n1 = graph
                .insert_between(
                    Content::new_simple("middle".to_string()),
                    graph.input.id().into(),
                    String::from("OL"),
                )
                .await?;

            // input -> [n0, n1] -> output
            assert_eq!(graph.len().await, 4);

            let path = graph.traverse_from_input_node().await;

            assert_eq!(path.len(), 4);

            Ok(())
        }

        #[tokio::test]
        async fn test_bfs_should_detect_path_between_nodes() -> Result<(), Box<dyn Error>> {
            let graph = CyclicGraph::new_default(
                String::from("IL"),
                Content::<String, String>::new_simple("input".to_string()),
                String::from("OL"),
                Content::new_simple("output".to_string()),
                0,
            )?;

            // input -> [n0, n1 -> n2] -> output

            let n0 = graph
                .insert_between(
                    Content::new_simple("middle".to_string()),
                    String::from("IL"),
                    String::from("OL"),
                )
                .await?;

            let n1 = graph
                .insert_between(
                    Content::new_simple("middle".to_string()),
                    String::from("IL"),
                    String::from("OL"),
                )
                .await?;

            let n2 = graph
                .insert_between(
                    Content::new_simple("middle".to_string()),
                    n1.id().into(),
                    String::from("OL"),
                )
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
