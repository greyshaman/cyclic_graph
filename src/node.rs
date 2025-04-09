use std::{borrow::Borrow, collections::HashSet, hash::Hash, sync::Arc};

use tokio::sync::RwLock;

/// A node in a graph with a set of ancestor and descendant nodes, a unique identifier,
/// and a payload that it is associated with.
/// I - the node identifier type
/// T - the type of node payload data
#[derive(Debug)]
pub struct Node<I, T> {
    /// The unique identifier
    id: I,

    /// The payload
    data: Arc<RwLock<T>>,

    /// The parent ids set
    parent_ids: RwLock<HashSet<I>>,

    /// The child ids set
    child_ids: RwLock<HashSet<I>>,
}

impl<I, T> Node<I, T>
where
    I: Clone + Eq + PartialEq + Hash,
{
    /// The node constructor
    ///
    /// # Example
    ///
    /// ```
    /// use cyclic_graph::Node;
    ///
    /// let node_i32 = Node::new(1, "one");
    /// let node_usize = Node::<usize, &str>::new(0, "zero");
    /// let node_string_id = Node::new(String::from("HL_0"), vec![0_usize, 1, 2]);
    /// ```
    pub fn new(id: I, data: T) -> Self {
        Self {
            id,
            data: Arc::new(RwLock::new(data)),
            parent_ids: RwLock::new(HashSet::new()),
            child_ids: RwLock::new(HashSet::new()),
        }
    }

    /// Returns id reference
    pub fn id(&self) -> &I {
        &self.id
    }

    /// Returns wrapped payload data
    pub fn data(&self) -> Arc<RwLock<T>> {
        self.data.clone()
    }

    /// Changes wrapped payload data
    pub async fn set_data(&self, value: T) {
        *self.data.write().await = value
    }

    /// Creates link to specified child and from child to current node as to parent
    pub async fn link_child(&self, child: Arc<Node<I, T>>) -> bool {
        self.child_ids.write().await.insert(child.id().clone())
            && child.parent_ids.write().await.insert(self.id().clone())
    }

    /// Removes links between child and current node as parent
    pub async fn unlink_child(&self, child: Arc<Node<I, T>>) -> bool {
        self.child_ids.write().await.remove(child.id())
            && child.parent_ids.write().await.remove(self.id())
    }

    /// Checks if current node has child node specified by id
    pub async fn has_child<Q>(&self, id: &Q) -> bool
    where
        I: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        self.child_ids.read().await.contains(id)
    }

    /// Checks if current node has connections to children
    pub async fn has_children(&self) -> bool {
        !self.child_ids.read().await.is_empty()
    }

    /// Returns vector with child ids
    pub async fn child_ids(&self) -> Vec<I> {
        self.child_ids
            .read()
            .await
            .iter()
            .cloned()
            .collect::<Vec<_>>()
    }
    /// Creates link to specified parent and from parent to current node as to child
    pub async fn link_parent(&self, parent: Arc<Node<I, T>>) -> bool {
        self.parent_ids.write().await.insert(parent.id().clone())
            && parent.child_ids.write().await.insert(self.id().clone())
    }

    /// Removes links between parent and current node as child
    pub async fn unlink_parent(&self, parent: Arc<Node<I, T>>) -> bool {
        self.parent_ids.write().await.remove(parent.id())
            && parent.child_ids.write().await.remove(self.id())
    }

    /// Checks if current node has parent node specified by id
    pub async fn has_parent<Q>(&self, id: &Q) -> bool
    where
        I: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        self.parent_ids.read().await.contains(id)
    }

    /// Checks if current node has connections to parents
    pub async fn has_parents(&self) -> bool {
        !self.parent_ids.read().await.is_empty()
    }

    /// Returns vector with parent ids
    pub async fn parent_ids(&self) -> Vec<I> {
        self.parent_ids
            .read()
            .await
            .iter()
            .cloned()
            .collect::<Vec<_>>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod for_id_as_usize {
        use super::*;

        #[tokio::test]
        async fn test_create_new_node() {
            let node = Node::new(0_usize, "test");
            let vec_node = Node::new(1_usize, vec![1_usize, 2, 3]);

            assert_eq!(node.id, 0);
            assert_eq!(node.data.read().await.clone(), "test");
            assert!(node.parent_ids.read().await.is_empty());
            assert!(node.child_ids.read().await.is_empty());

            assert_eq!(vec_node.id, 1);
            assert_eq!(vec_node.data().read().await.len(), 3);
        }

        #[test]
        fn test_id_accessor_should_return_correct_id_value() {
            let node = Node::new(0_usize, "test");

            assert_eq!(node.id(), &0);
        }

        #[tokio::test]
        async fn test_data_accessor_should_return_correct_data_ref() {
            let node = Node::new(0_usize, "test");

            assert_eq!(node.data().read().await.clone(), "test");
        }

        #[tokio::test]
        async fn test_data_mut_accessor_allowed_to_change_node_data() {
            let node = Node::new(0_usize, "test");

            {
                let data = node.data();
                let mut data_mut = data.write().await;
                *data_mut = "new test";
            }

            assert_eq!(node.data().read().await.clone(), "new test");
        }

        #[tokio::test]
        async fn test_set_data_should_change_node_data() {
            let node = Node::new(0_usize, "test");

            node.set_data("new test").await;

            assert_eq!(node.data().read().await.clone(), "new test");
        }

        #[tokio::test]
        async fn test_linking_two_nodes_by_link_child() {
            let root_node = Arc::new(Node::new(0_usize, "root"));
            let child_node = Arc::new(Node::new(1_usize, "child"));

            assert!(root_node.link_child(child_node.clone()).await);

            assert!(!root_node.has_parents().await);
            assert!(root_node.has_children().await);
            assert!(child_node.has_parents().await);
            assert!(!child_node.has_children().await);
        }

        #[tokio::test]
        async fn test_second_link_child_attempt_should_return_false() {
            let parent = Arc::new(Node::new(0_usize, "parent"));
            let child = Arc::new(Node::new(1_usize, "child"));

            assert!(parent.link_child(child.clone()).await);
            assert!(!parent.link_child(child.clone()).await);
        }

        #[tokio::test]
        async fn test_second_link_parent_attempt_should_return_false() {
            let parent = Arc::new(Node::new(0_usize, "parent"));
            let child = Arc::new(Node::new(1_usize, "child"));

            assert!(child.link_parent(parent.clone()).await);
            assert!(!child.link_parent(parent.clone()).await);
        }

        #[tokio::test]
        async fn test_unlink_child_should_break_link_between_linked_nodes() {
            let parent = Arc::new(Node::new(0_usize, "parent"));
            let child = Arc::new(Node::new(1_usize, "child"));

            assert!(parent.link_child(child.clone()).await);

            assert!(parent.unlink_child(child.clone()).await);

            assert!(!parent.has_children().await);
            assert!(!child.has_parents().await);
        }

        #[tokio::test]
        async fn test_second_unlink_child_should_return_false() {
            let parent = Arc::new(Node::new(0_usize, "parent"));
            let child = Arc::new(Node::new(1_usize, "child"));

            assert!(parent.link_child(child.clone()).await);

            assert!(parent.unlink_child(child.clone()).await);
            assert!(!parent.unlink_child(child.clone()).await);
        }

        #[tokio::test]
        async fn test_unlink_parent_should_break_link_between_linked_nodes() {
            let parent = Arc::new(Node::new(0_usize, "parent"));
            let child = Arc::new(Node::new(1_usize, "child"));

            assert!(parent.link_child(child.clone()).await);

            assert!(child.unlink_parent(parent.clone()).await);

            assert!(!parent.has_children().await);
            assert!(!child.has_parents().await);
        }

        #[tokio::test]
        async fn test_second_unlink_parent_should_return_false() {
            let parent = Arc::new(Node::new(0_usize, "parent"));
            let child = Arc::new(Node::new(1_usize, "child"));

            assert!(child.link_parent(parent.clone()).await);

            assert!(child.unlink_parent(parent.clone()).await);
            assert!(!child.unlink_parent(parent.clone()).await);
        }

        #[tokio::test]
        async fn test_child_ids_should_return_children_identifiers_in_vector() {
            let parent = Arc::new(Node::new(0_usize, "parent"));

            let child1 = Arc::new(Node::new(1_usize, "child1"));
            let child2 = Arc::new(Node::new(2_usize, "child2"));

            assert!(parent.link_child(child1.clone()).await);
            assert!(parent.link_child(child2.clone()).await);

            let ids = parent.child_ids().await;

            assert!(!ids.is_empty());
            assert_eq!(ids.len(), 2);
            assert!(ids.contains(&1));
            assert!(ids.contains(&2));
        }

        #[tokio::test]
        async fn test_children_ids_should_return_children_identifiers_in_vector() {
            let parent1 = Arc::new(Node::new(0_usize, "parent"));
            let parent2 = Arc::new(Node::new(1_usize, "parent"));

            let child = Arc::new(Node::new(2_usize, "child1"));

            assert!(parent1.link_child(child.clone()).await);
            assert!(parent2.link_child(child.clone()).await);

            let ids = child.parent_ids().await;

            assert!(!ids.is_empty());
            assert_eq!(ids.len(), 2);
            assert!(ids.contains(&0));
            assert!(ids.contains(&1));
        }

        #[tokio::test]
        async fn test_linking_two_nodes_by_link_parent() {
            let root_node = Arc::new(Node::new(0_usize, "root"));
            let child_node = Arc::new(Node::new(1_usize, "child"));

            assert!(child_node.link_parent(root_node.clone()).await);

            assert!(!root_node.has_parents().await);
            assert!(root_node.has_children().await);
            assert!(child_node.has_parents().await);
            assert!(!child_node.has_children().await);
        }

        #[tokio::test]
        async fn test_has_child_should_return_true_with_correct_child_id() {
            let node0 = Arc::new(Node::new(0_usize, "root"));
            let node1 = Arc::new(Node::new(1_usize, "child"));

            node0.link_child(node1.clone()).await;

            assert!(node0.has_child(&1).await);
        }

        #[tokio::test]
        async fn test_has_child_should_return_false_with_incorrect_child_id() {
            let node = Node::new(0_usize, "test");

            assert!(!node.has_child(&1).await);
        }

        #[tokio::test]
        async fn test_has_parent_should_return_true_with_correct_parent_id() {
            let node0 = Arc::new(Node::new(0_usize, "n0"));
            let node1 = Arc::new(Node::new(1_usize, "n1"));

            node1.link_parent(node0.clone()).await;

            assert!(node1.has_parent(&0).await);
        }

        #[tokio::test]
        async fn test_has_parent_should_return_false_with_incorrect_parent_id() {
            let node = Node::new(0_usize, "test");

            assert!(!node.has_parent(&1).await);
        }

        #[tokio::test]
        async fn test_allow_link_self_node_as_child() {
            let node = Arc::new(Node::new(0_usize, "test"));

            node.link_child(node.clone()).await;

            assert!(node.has_child(node.id()).await);
        }
    }

    mod for_id_as_str {
        use super::*;

        #[tokio::test]
        async fn test_create_new_node() {
            let node = Node::new("IL", "test");
            let vec_node = Node::new("IL", vec![1, 2, 3]);

            assert_eq!(node.id, "IL");
            assert_eq!(node.data.read().await.clone(), "test");
            assert!(node.parent_ids.read().await.is_empty());
            assert!(node.child_ids.read().await.is_empty());

            assert_eq!(vec_node.id, "IL");
            assert_eq!(vec_node.data().read().await.len(), 3);
        }

        #[test]
        fn test_id_accessor_should_return_correct_id_value() {
            let node = Node::new("IL", "test");

            assert_eq!(node.id(), &"IL");
        }

        #[tokio::test]
        async fn test_data_accessor_should_return_correct_data_ref() {
            let node = Node::new("IL", "test");

            assert_eq!(node.data().read().await.clone(), "test");
        }

        #[tokio::test]
        async fn test_data_mut_accessor_allowed_to_change_node_data() {
            let node = Node::new("IL", "test");

            {
                let data = node.data();
                let mut data_mut = data.write().await;
                *data_mut = "new test";
            }

            assert_eq!(node.data().read().await.clone(), "new test");
        }

        #[tokio::test]
        async fn test_set_data_should_change_node_data() {
            let node = Node::new("IL", "test");

            node.set_data("new test").await;

            assert_eq!(node.data().read().await.clone(), "new test");
        }

        #[tokio::test]
        async fn test_linking_two_nodes_by_link_child() {
            let root_node = Arc::new(Node::new("n0", "root"));
            let child_node = Arc::new(Node::new("n1", "child"));

            assert!(root_node.link_child(child_node.clone()).await);

            assert!(!root_node.has_parents().await);
            assert!(root_node.has_children().await);
            assert!(child_node.has_parents().await);
            assert!(!child_node.has_children().await);
        }

        #[tokio::test]
        async fn test_second_link_child_attempt_should_return_false() {
            let parent = Arc::new(Node::new("n0", "parent"));
            let child = Arc::new(Node::new("n1", "child"));

            assert!(parent.link_child(child.clone()).await);
            assert!(!parent.link_child(child.clone()).await);
        }

        #[tokio::test]
        async fn test_second_link_parent_attempt_should_return_false() {
            let parent = Arc::new(Node::new("n0", "parent"));
            let child = Arc::new(Node::new("n1", "child"));

            assert!(child.link_parent(parent.clone()).await);
            assert!(!child.link_parent(parent.clone()).await);
        }

        #[tokio::test]
        async fn test_unlink_child_should_break_link_between_linked_nodes() {
            let parent = Arc::new(Node::new("n0", "parent"));
            let child = Arc::new(Node::new("n1", "child"));

            assert!(parent.link_child(child.clone()).await);

            assert!(parent.unlink_child(child.clone()).await);

            assert!(!parent.has_children().await);
            assert!(!child.has_parents().await);
        }

        #[tokio::test]
        async fn test_second_unlink_child_should_return_false() {
            let parent = Arc::new(Node::new("n0", "parent"));
            let child = Arc::new(Node::new("n1", "child"));

            assert!(parent.link_child(child.clone()).await);

            assert!(parent.unlink_child(child.clone()).await);
            assert!(!parent.unlink_child(child.clone()).await);
        }

        #[tokio::test]
        async fn test_unlink_parent_should_break_link_between_linked_nodes() {
            let parent = Arc::new(Node::new("n0", "parent"));
            let child = Arc::new(Node::new("n1", "child"));

            assert!(parent.link_child(child.clone()).await);

            assert!(child.unlink_parent(parent.clone()).await);

            assert!(!parent.has_children().await);
            assert!(!child.has_parents().await);
        }

        #[tokio::test]
        async fn test_second_unlink_parent_should_return_false() {
            let parent = Arc::new(Node::new("n0", "parent"));
            let child = Arc::new(Node::new("n1", "child"));

            assert!(child.link_parent(parent.clone()).await);

            assert!(child.unlink_parent(parent.clone()).await);
            assert!(!child.unlink_parent(parent.clone()).await);
        }

        #[tokio::test]
        async fn test_child_ids_should_return_children_identifiers_in_vector() {
            let parent = Arc::new(Node::new("n0", "parent"));

            let child1 = Arc::new(Node::new("n1", "child1"));
            let child2 = Arc::new(Node::new("n2", "child2"));

            assert!(parent.link_child(child1.clone()).await);
            assert!(parent.link_child(child2.clone()).await);

            let ids = parent.child_ids().await;

            assert!(!ids.is_empty());
            assert_eq!(ids.len(), 2);
            assert!(ids.contains(&"n1"));
            assert!(ids.contains(&"n2"));
        }

        #[tokio::test]
        async fn test_children_ids_should_return_children_identifiers_in_vector() {
            let parent1 = Arc::new(Node::new("n0", "parent"));
            let parent2 = Arc::new(Node::new("n1", "parent"));

            let child = Arc::new(Node::new("n2", "child1"));

            assert!(parent1.link_child(child.clone()).await);
            assert!(parent2.link_child(child.clone()).await);

            let ids = child.parent_ids().await;

            assert!(!ids.is_empty());
            assert_eq!(ids.len(), 2);
            assert!(ids.contains(&"n0"));
            assert!(ids.contains(&"n1"));
        }

        #[tokio::test]
        async fn test_linking_two_nodes_by_link_parent() {
            let root_node = Arc::new(Node::new("n0", "root"));
            let child_node = Arc::new(Node::new("n1", "child"));

            assert!(child_node.link_parent(root_node.clone()).await);

            assert!(!root_node.has_parents().await);
            assert!(root_node.has_children().await);
            assert!(child_node.has_parents().await);
            assert!(!child_node.has_children().await);
        }

        #[tokio::test]
        async fn test_has_child_should_return_true_with_correct_child_id() {
            let node0 = Arc::new(Node::new("n0", "root"));
            let node1 = Arc::new(Node::new("n1", "child"));

            node0.link_child(node1.clone()).await;

            assert!(node0.has_child(&"n1").await);
        }

        #[tokio::test]
        async fn test_has_child_should_return_false_with_incorrect_child_id() {
            let node = Node::new("n0", "test");

            assert!(!node.has_child(&"n1").await);
        }

        #[tokio::test]
        async fn test_has_parent_should_return_true_with_correct_parent_id() {
            let node0 = Arc::new(Node::new("n0", "n0"));
            let node1 = Arc::new(Node::new("n1", "n1"));

            node1.link_parent(node0.clone()).await;

            assert!(node1.has_parent(&"n0").await);
        }

        #[tokio::test]
        async fn test_has_parent_should_return_false_with_incorrect_parent_id() {
            let node = Node::new("n0", "test");

            assert!(!node.has_parent(&"n1").await);
        }

        #[tokio::test]
        async fn test_allow_link_self_node_as_child() {
            let node = Arc::new(Node::new("n0", "test"));

            node.link_child(node.clone()).await;

            assert!(node.has_child(node.id()).await);
        }
    }
}
