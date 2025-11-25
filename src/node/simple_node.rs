use std::collections::HashSet;
use std::fmt::Debug;
use std::hash::Hash;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::TimeBasedId;
use crate::weak_node_wrapper::WeakNodeWrapper;

use super::Node;

#[derive(Debug)]
pub struct SimpleNode<D, I = TimeBasedId>
where
    D: 'static + Send + Sync + Clone + Debug + Default,
    I: 'static + Send + Sync + Clone + Eq + PartialEq + Hash + Debug + Default,
{
    id: I,
    data: Arc<RwLock<D>>,
    parents: Arc<RwLock<HashSet<WeakNodeWrapper<SimpleNode<D, I>>>>>,
    children: Arc<RwLock<HashSet<Arc<SimpleNode<D, I>>>>>,
}

impl<D, I> PartialEq for SimpleNode<D, I>
where
    D: 'static + Send + Sync + Clone + Debug + Default,
    I: 'static + Send + Sync + Clone + Eq + PartialEq + Hash + Debug + Default,
{
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<D, I> Eq for SimpleNode<D, I>
where
    D: 'static + Send + Sync + Clone + Debug + Default,
    I: 'static + Send + Sync + Clone + Eq + PartialEq + Hash + Debug + Default,
{
}

impl<D, I> Hash for SimpleNode<D, I>
where
    D: 'static + Send + Sync + Clone + Debug + Default,
    I: 'static + Send + Sync + Clone + Eq + PartialEq + Hash + Debug + Default,
{
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl<D, I> SimpleNode<D, I>
where
    D: 'static + Send + Sync + Clone + Debug + Default,
    I: 'static + Send + Sync + Clone + Eq + PartialEq + Hash + Debug + Default,
{
    pub fn new(id: I, data: D) -> Self {
        Self {
            id,
            data: Arc::new(RwLock::new(data)),
            parents: Arc::new(RwLock::new(HashSet::new())),
            children: Arc::new(RwLock::new(HashSet::new())),
        }
    }
}

impl<D> Default for SimpleNode<D, TimeBasedId>
where
    D: 'static + Send + Sync + Clone + Debug + Default,
{
    fn default() -> Self {
        Self::new(TimeBasedId::default(), D::default())
    }
}

#[async_trait]
impl<D, I> Node for SimpleNode<D, I>
where
    D: 'static + Send + Sync + Clone + Debug + Default,
    I: 'static + Send + Sync + Clone + Eq + PartialEq + Hash + Debug + Default,
{
    type Index = I;

    type Data = D;

    type Signal = ();

    type NeighborNode = SimpleNode<D, I>;

    /// Returns id
    fn id(&self) -> Self::Index {
        self.id.clone()
    }

    /// Returns wrapped payload value of the node's content.
    /// Returns None if Node's content is not contained payload.
    async fn value(&self) -> Option<Self::Data> {
        let r_data = self.data.read().await;
        Some(r_data.clone())
    }

    /// Sets the value to the node's payload
    async fn set_value(&self, new_value: Self::Data) -> Option<Self::Data> {
        let mut w_data = self.data.write().await;
        let old_value = Some(w_data.clone());
        *w_data = new_value;
        old_value
    }

    fn children(&self) -> Arc<RwLock<HashSet<Arc<SimpleNode<D, I>>>>> {
        self.children.clone()
    }

    fn parents(&self) -> Arc<RwLock<HashSet<WeakNodeWrapper<SimpleNode<D, I>>>>> {
        self.parents.clone()
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::SimplexStream;

    use crate::node;

    use super::*;

    #[test]
    fn test_create_simple_node_with_i32_id() {
        let node = SimpleNode::new(1, "Hello".to_string());
        assert_eq!(node.id(), 1);
    }

    #[test]
    fn test_create_simple_node_with_timebased_id_generator() {
        let node = SimpleNode::<String, TimeBasedId>::default();
        assert!(node.id().value() > 0);
    }

    #[tokio::test]
    async fn test_value_accessors() {
        let node = SimpleNode::<&str, TimeBasedId>::default();
        node.set_value("Test").await;

        assert_eq!(node.value().await, Some("Test"));
    }

    #[tokio::test]
    async fn test_has_children_for_new_node_should_be_false() {
        let node = SimpleNode::<String, TimeBasedId>::default();
        assert!(!node.has_children().await);
    }

    #[tokio::test]
    async fn test_has_parents_for_new_node_should_be_false() {
        let node = SimpleNode::<String, TimeBasedId>::default();
        assert!(!node.has_parents().await);
    }

    #[tokio::test]
    async fn test_link_child() {
        let node1 = SimpleNode::<String, TimeBasedId>::default();
        let node2 = SimpleNode::<String, TimeBasedId>::default();
        let result = node1.link_child(Arc::new(node2)).await;
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_link_parent() {
        let node1 = SimpleNode::<String, TimeBasedId>::default();
        let node2 = SimpleNode::<String, TimeBasedId>::default();
        let result = node2.link_parent(Arc::new(node1)).await;
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_has_children_for_linked_parent_node_should_return_true() {
        let parent_node = SimpleNode::<String, TimeBasedId>::default();
        let child_node = SimpleNode::<String, TimeBasedId>::default();
        let result = parent_node.link_child(Arc::new(child_node)).await;
        assert!(result.is_ok());
        assert!(parent_node.has_children().await);
    }

    #[tokio::test]
    async fn test_has_parents_for_linked_child_node_should_return_true() {
        let parent_node = SimpleNode::<String, TimeBasedId>::default();
        let child_node = SimpleNode::<String, TimeBasedId>::default();
        let result = child_node.link_parent(Arc::new(parent_node)).await;
        assert!(result.is_ok());
        assert!(child_node.has_parents().await);
    }

    #[tokio::test]
    async fn test_link_node_as_child_to_itself_should_return_error() {
        let node = SimpleNode::<String, TimeBasedId>::default();
        let arc_node = Arc::new(node);

        let res = arc_node.link_child(arc_node.clone()).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_link_node_as_parent_to_itself_should_return_error() {
        let node = SimpleNode::<String, TimeBasedId>::default();
        let arc_node = Arc::new(node);

        let res = arc_node.link_parent(arc_node.clone()).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_has_child_with_id_method_for_new_node_should_return_false() {
        let node = SimpleNode::new(1, "one");
        assert!(!node.has_child_with_id(&2).await);
    }

    #[tokio::test]
    async fn test_has_child_with_id_with_existing_child_node_should_return_true() {
        let parent_node = Arc::new(SimpleNode::new(1, "one"));
        let child_node = Arc::new(SimpleNode::new(2, "two"));

        assert!(parent_node.link_child(child_node.clone()).await.is_ok());
        assert!(parent_node.has_child_with_id(&child_node.id()).await);
    }

    #[tokio::test]
    async fn test_child_ids_method_should_contains_id_of_linked_child() {
        let parent_node = Arc::new(SimpleNode::new(1, "one"));
        let child_node = Arc::new(SimpleNode::new(2, "two"));

        let _not_linked_node = Arc::new(SimpleNode::new(3, "three"));

        assert!(parent_node.link_child(child_node.clone()).await.is_ok());
        let child_ids = parent_node.child_ids().await;

        assert!(child_ids.contains(&2));
        assert!(!child_ids.contains(&3));
    }

    /// Test successor_ids method for cyclic graph for each node
    /// Graph example:
    /// 1 -> [2 -> [3 -> 4 -> [2, 5 -> [3, 9]]], 6 -> 7 -> [5, 9], 8 -> 9]
    #[tokio::test]
    async fn test_successor_ids_method_should_contains_id_of_successors() {
        let node1 = Arc::new(SimpleNode::<()>::default());
        let node2 = Arc::new(SimpleNode::<()>::default());
        let node3 = Arc::new(SimpleNode::<()>::default());
        let node4 = Arc::new(SimpleNode::<()>::default());
        let node5 = Arc::new(SimpleNode::<()>::default());
        let node6 = Arc::new(SimpleNode::<()>::default());
        let node7 = Arc::new(SimpleNode::<()>::default());
        let node8 = Arc::new(SimpleNode::<()>::default());
        let node9 = Arc::new(SimpleNode::<()>::default());

        // let node1 = Arc::new(SimpleNode::new(1, ()));
        // let node2 = Arc::new(SimpleNode::new(2, ()));
        // let node3 = Arc::new(SimpleNode::new(3, ()));
        // let node4 = Arc::new(SimpleNode::new(4, ()));
        // let node5 = Arc::new(SimpleNode::new(5, ()));
        // let node6 = Arc::new(SimpleNode::new(6, ()));
        // let node7 = Arc::new(SimpleNode::new(7, ()));
        // let node8 = Arc::new(SimpleNode::new(8, ()));
        // let node9 = Arc::new(SimpleNode::new(9, ()));

        // 1 -> 2
        // 1 -> 6
        // 1 -> 8
        assert!(node1.link_child(node2.clone()).await.is_ok());
        assert!(node1.link_child(node6.clone()).await.is_ok());
        assert!(node1.link_child(node8.clone()).await.is_ok());

        // 2 -> 3
        assert!(node2.link_child(node3.clone()).await.is_ok());

        // 3 -> 4
        assert!(node3.link_child(node4.clone()).await.is_ok());

        // 4 -> 2
        // 4 -> 5
        assert!(node4.link_child(node2.clone()).await.is_ok());
        assert!(node4.link_child(node5.clone()).await.is_ok());

        // 5 -> 3
        // 5 -> 9
        assert!(node5.link_child(node3.clone()).await.is_ok());
        assert!(node5.link_child(node9.clone()).await.is_ok());

        // 6 -> 7
        assert!(node6.link_child(node7.clone()).await.is_ok());

        // 7 -> 5
        // 7 -> 9
        assert!(node7.link_child(node5.clone()).await.is_ok());
        assert!(node7.link_child(node9.clone()).await.is_ok());

        // 8 -> 9
        assert!(node8.link_child(node9.clone()).await.is_ok());

        // for 2 is [2, 3, 4, 5, 9]
        let successor_ids = node1.successor_ids(&HashSet::new()).await;
        assert!(!successor_ids.contains(&node1.id()));
        assert!(successor_ids.contains(&node2.id()));
        assert!(successor_ids.contains(&node3.id()));
        assert!(successor_ids.contains(&node4.id()));
        assert!(successor_ids.contains(&node5.id()));
        assert!(successor_ids.contains(&node6.id()));
        assert!(successor_ids.contains(&node7.id()));
        assert!(successor_ids.contains(&node8.id()));
        assert!(successor_ids.contains(&node9.id()));

        // for 2 is [2, 3, 4, 5, 9]
        let successor_ids = node2.successor_ids(&HashSet::new()).await;
        assert!(successor_ids.contains(&node2.id()));
        assert!(successor_ids.contains(&node3.id()));
        assert!(successor_ids.contains(&node4.id()));
        assert!(successor_ids.contains(&node5.id()));
        assert!(successor_ids.contains(&node9.id()));
        assert_eq!(successor_ids.len(), 5);

        // for 3 is [4, 2, 3, 5, 9]
        let successor_ids = node3.successor_ids(&HashSet::new()).await;
        assert!(successor_ids.contains(&node2.id()));
        assert!(successor_ids.contains(&node3.id()));
        assert!(successor_ids.contains(&node4.id()));
        assert!(successor_ids.contains(&node5.id()));
        assert!(successor_ids.contains(&node9.id()));
        assert_eq!(successor_ids.len(), 5);

        // for 4 is [2, 3, 4, 5, 9]
        let successor_ids = node4.successor_ids(&HashSet::new()).await;
        assert!(successor_ids.contains(&node2.id()));
        assert!(successor_ids.contains(&node3.id()));
        assert!(successor_ids.contains(&node4.id()));
        assert!(successor_ids.contains(&node5.id()));
        assert!(successor_ids.contains(&node9.id()));
        assert_eq!(successor_ids.len(), 5);

        // for 5 is [3, 4, 2, 5, 9]
        let successor_ids = node5.successor_ids(&HashSet::new()).await;
        assert!(successor_ids.contains(&node2.id()));
        assert!(successor_ids.contains(&node3.id()));
        assert!(successor_ids.contains(&node4.id()));
        assert!(successor_ids.contains(&node5.id()));
        assert!(successor_ids.contains(&node9.id()));
        assert_eq!(successor_ids.len(), 5);

        // for 6 is [7, 5, 3, 4, 2, 9]
        let successor_ids = node6.successor_ids(&HashSet::new()).await;
        assert!(successor_ids.contains(&node2.id()));
        assert!(successor_ids.contains(&node3.id()));
        assert!(successor_ids.contains(&node4.id()));
        assert!(successor_ids.contains(&node5.id()));
        assert!(successor_ids.contains(&node7.id()));
        assert!(successor_ids.contains(&node9.id()));
        assert_eq!(successor_ids.len(), 6);

        // for 7 is [5, 3, 4, 2, 9]
        let successor_ids = node7.successor_ids(&HashSet::new()).await;
        assert!(successor_ids.contains(&node2.id()));
        assert!(successor_ids.contains(&node3.id()));
        assert!(successor_ids.contains(&node4.id()));
        assert!(successor_ids.contains(&node5.id()));
        assert!(successor_ids.contains(&node9.id()));
        assert_eq!(successor_ids.len(), 5);

        // for 8 is [9]
        let successor_ids = node8.successor_ids(&HashSet::new()).await;
        assert_eq!(successor_ids.len(), 1);
        assert!(successor_ids.contains(&node9.id()));

        // for 9 is []
        let successor_ids = node9.successor_ids(&HashSet::new()).await;
        assert!(successor_ids.is_empty());
    }

    /// Test successor_ids method for cyclic graph for each node
    /// Graph example:
    /// 1 -> [2 -> [3 -> 4 -> [2, 5 -> [3, 9]]], 6 -> 7 -> [5, 9], 8 -> 9]
    #[tokio::test]
    async fn test_predecessor_ids_method_should_contains_id_of_predecessors() {
        // let node1 = Arc::new(SimpleNode::<()>::default());
        // let node2 = Arc::new(SimpleNode::<()>::default());
        // let node3 = Arc::new(SimpleNode::<()>::default());
        // let node4 = Arc::new(SimpleNode::<()>::default());
        // let node5 = Arc::new(SimpleNode::<()>::default());
        // let node6 = Arc::new(SimpleNode::<()>::default());
        // let node7 = Arc::new(SimpleNode::<()>::default());
        // let node8 = Arc::new(SimpleNode::<()>::default());
        // let node9 = Arc::new(SimpleNode::<()>::default());

        let node1 = Arc::new(SimpleNode::new(1, ()));
        let node2 = Arc::new(SimpleNode::new(2, ()));
        let node3 = Arc::new(SimpleNode::new(3, ()));
        let node4 = Arc::new(SimpleNode::new(4, ()));
        let node5 = Arc::new(SimpleNode::new(5, ()));
        let node6 = Arc::new(SimpleNode::new(6, ()));
        let node7 = Arc::new(SimpleNode::new(7, ()));
        let node8 = Arc::new(SimpleNode::new(8, ()));
        let node9 = Arc::new(SimpleNode::new(9, ()));

        // 1 -> 2
        // 1 -> 6
        // 1 -> 8
        assert!(node2.link_parent(node1.clone()).await.is_ok());
        assert!(node6.link_parent(node1.clone()).await.is_ok());
        assert!(node8.link_parent(node1.clone()).await.is_ok());

        // 2 -> 3
        assert!(node3.link_parent(node2.clone()).await.is_ok());

        // 3 -> 4
        assert!(node4.link_parent(node3.clone()).await.is_ok());

        // 4 -> 2
        // 4 -> 5
        assert!(node2.link_parent(node4.clone()).await.is_ok());
        assert!(node5.link_parent(node4.clone()).await.is_ok());

        // 5 -> 3
        // 5 -> 9
        assert!(node3.link_parent(node5.clone()).await.is_ok());
        assert!(node9.link_parent(node5.clone()).await.is_ok());

        // 6 -> 7
        assert!(node7.link_parent(node6.clone()).await.is_ok());

        // 7 -> 5
        // 7 -> 9
        assert!(node5.link_parent(node7.clone()).await.is_ok());
        assert!(node9.link_parent(node7.clone()).await.is_ok());

        // 8 -> 9
        assert!(node9.link_parent(node8.clone()).await.is_ok());

        // for 1 is []
        let predecessor_ids = node1.predecessor_ids(&HashSet::new()).await;
        assert!(predecessor_ids.is_empty());

        // for 2 is [1, 2, 3, 4, 5, 6, 7]
        let predecessor_ids = node2.predecessor_ids(&HashSet::new()).await;
        assert!(predecessor_ids.contains(&node1.id()));
        assert!(predecessor_ids.contains(&node2.id()));
        assert!(predecessor_ids.contains(&node3.id()));
        assert!(predecessor_ids.contains(&node4.id()));
        assert!(predecessor_ids.contains(&node5.id()));
        assert!(predecessor_ids.contains(&node6.id()));
        assert!(predecessor_ids.contains(&node7.id()));
        assert_eq!(predecessor_ids.len(), 7);

        // for 3 is [1, 2, 3, 4, 5, 6, 7]
        let predecessor_ids = node3.predecessor_ids(&HashSet::new()).await;
        assert!(predecessor_ids.contains(&node1.id()));
        assert!(predecessor_ids.contains(&node2.id()));
        assert!(predecessor_ids.contains(&node3.id()));
        assert!(predecessor_ids.contains(&node4.id()));
        assert!(predecessor_ids.contains(&node5.id()));
        assert!(predecessor_ids.contains(&node6.id()));
        assert!(predecessor_ids.contains(&node7.id()));
        assert_eq!(predecessor_ids.len(), 7);

        // for 4 is [1, 2, 3, 4, 5, 6, 7]
        let predecessor_ids = node4.predecessor_ids(&HashSet::new()).await;
        assert!(predecessor_ids.contains(&node1.id()));
        assert!(predecessor_ids.contains(&node2.id()));
        assert!(predecessor_ids.contains(&node3.id()));
        assert!(predecessor_ids.contains(&node4.id()));
        assert!(predecessor_ids.contains(&node5.id()));
        assert!(predecessor_ids.contains(&node6.id()));
        assert!(predecessor_ids.contains(&node7.id()));
        assert_eq!(predecessor_ids.len(), 7);

        // for 5 is [1, 2, 3, 4, 5, 6, 7]
        let predecessor_ids = node5.predecessor_ids(&HashSet::new()).await;
        assert!(predecessor_ids.contains(&node1.id()));
        assert!(predecessor_ids.contains(&node2.id()));
        assert!(predecessor_ids.contains(&node3.id()));
        assert!(predecessor_ids.contains(&node4.id()));
        assert!(predecessor_ids.contains(&node5.id()));
        assert!(predecessor_ids.contains(&node6.id()));
        assert!(predecessor_ids.contains(&node7.id()));
        assert_eq!(predecessor_ids.len(), 7);

        // for 6 is [1]
        let predecessor_ids = node6.predecessor_ids(&HashSet::new()).await;
        assert!(predecessor_ids.contains(&node1.id()));
        assert_eq!(predecessor_ids.len(), 1);

        // for 7 is [1, 6]
        let predecessor_ids = node7.predecessor_ids(&HashSet::new()).await;
        assert!(predecessor_ids.contains(&node1.id()));
        assert!(predecessor_ids.contains(&node6.id()));
        assert_eq!(predecessor_ids.len(), 2);

        // for 8 is [1]
        let predecessor_ids = node8.predecessor_ids(&HashSet::new()).await;
        assert!(predecessor_ids.contains(&node1.id()));
        assert_eq!(predecessor_ids.len(), 1);

        // for 9 is [1, 2, 3, 4, 5, 6, 7, 8]
        let predecessor_ids = node9.predecessor_ids(&HashSet::new()).await;
        assert!(predecessor_ids.contains(&node1.id()));
        assert!(predecessor_ids.contains(&node2.id()));
        assert!(predecessor_ids.contains(&node3.id()));
        assert!(predecessor_ids.contains(&node4.id()));
        assert!(predecessor_ids.contains(&node5.id()));
        assert!(predecessor_ids.contains(&node6.id()));
        assert!(predecessor_ids.contains(&node7.id()));
        assert!(predecessor_ids.contains(&node8.id()));
        assert_eq!(predecessor_ids.len(), 8);
    }
}
