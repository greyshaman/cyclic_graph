use std::{collections::HashSet, sync::Arc};

use tokio::sync::RwLock;

#[derive(Debug)]
pub struct Node<T> {
    id: usize,
    data: Arc<RwLock<T>>,
    parent_ids: RwLock<HashSet<usize>>,
    child_ids: RwLock<HashSet<usize>>,
}

impl<T> Node<T> {
    pub fn new(id: usize, data: T) -> Self {
        Self {
            id,
            data: Arc::new(RwLock::new(data)),
            parent_ids: RwLock::new(HashSet::new()),
            child_ids: RwLock::new(HashSet::new()),
        }
    }

    pub fn id(&self) -> usize {
        self.id
    }

    pub fn data(&self) -> Arc<RwLock<T>> {
        self.data.clone()
    }

    pub async fn set_data(&self, value: T) {
        *self.data.write().await = value
    }

    pub async fn link_child(&self, child: Arc<Node<T>>) -> bool {
        self.child_ids.write().await.insert(child.id)
            && child.parent_ids.write().await.insert(self.id)
    }

    pub async fn unlink_child(&self, child: Arc<Node<T>>) -> bool {
        self.child_ids.write().await.remove(&child.id)
            && child.parent_ids.write().await.remove(&self.id)
    }

    pub async fn has_child(&self, id: usize) -> bool {
        self.child_ids.read().await.contains(&id)
    }

    pub async fn has_children(&self) -> bool {
        !self.child_ids.read().await.is_empty()
    }

    pub async fn child_ids(&self) -> Vec<usize> {
        self.child_ids
            .read()
            .await
            .iter()
            .map(|id| id.clone())
            .collect::<Vec<usize>>()
    }

    pub async fn link_parent(&self, parent: Arc<Node<T>>) -> bool {
        self.parent_ids.write().await.insert(parent.id)
            && parent.child_ids.write().await.insert(self.id)
    }

    pub async fn unlink_parent(&self, parent: Arc<Node<T>>) -> bool {
        self.parent_ids.write().await.remove(&parent.id)
            && parent.child_ids.write().await.remove(&self.id)
    }

    pub async fn has_parent(&self, id: usize) -> bool {
        self.parent_ids.read().await.contains(&id)
    }

    pub async fn has_parents(&self) -> bool {
        !self.parent_ids.read().await.is_empty()
    }

    pub async fn parent_ids(&self) -> Vec<usize> {
        self.parent_ids
            .read()
            .await
            .iter()
            .map(|id| id.clone())
            .collect::<Vec<usize>>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_new_node() {
        let node = Node::new(0, "test");
        let vec_node = Node::new(1, vec![1, 2, 3]);

        assert_eq!(node.id, 0);
        assert_eq!(node.data.read().await.clone(), "test");
        assert!(node.parent_ids.read().await.is_empty());
        assert!(node.child_ids.read().await.is_empty());

        assert_eq!(vec_node.id(), 1);
        assert_eq!(vec_node.data().read().await.len(), 3);
    }

    #[test]
    fn test_id_accessor_should_return_correct_id_value() {
        let node = Node::new(0, "test");

        assert_eq!(node.id(), 0);
    }

    #[tokio::test]
    async fn test_data_accessor_should_return_correct_data_ref() {
        let node = Node::new(0, "test");

        assert_eq!(node.data().read().await.clone(), "test");
    }

    #[tokio::test]
    async fn test_data_mut_accessor_allowed_to_change_node_data() {
        let node = Node::new(0, "test");

        {
            let data = node.data();
            let mut data_mut = data.write().await;
            *data_mut = "new test";
        }

        assert_eq!(node.data().read().await.clone(), "new test");
    }

    #[tokio::test]
    async fn test_set_data_should_change_node_data() {
        let node = Node::new(0, "test");

        node.set_data("new test").await;

        assert_eq!(node.data().read().await.clone(), "new test");
    }

    #[tokio::test]
    async fn test_linking_two_nodes_by_link_child() {
        let root_node = Arc::new(Node::new(0, "root"));
        let child_node = Arc::new(Node::new(1, "child"));

        assert!(root_node.link_child(child_node.clone()).await);

        assert!(!root_node.has_parents().await);
        assert!(root_node.has_children().await);
        assert!(child_node.has_parents().await);
        assert!(!child_node.has_children().await);
    }

    #[tokio::test]
    async fn test_second_link_child_attempt_should_return_false() {
        let parent = Arc::new(Node::new(0, "parent"));
        let child = Arc::new(Node::new(1, "child"));

        assert!(parent.link_child(child.clone()).await);
        assert!(!parent.link_child(child.clone()).await);
    }

    #[tokio::test]
    async fn test_second_link_parent_attempt_should_return_false() {
        let parent = Arc::new(Node::new(0, "parent"));
        let child = Arc::new(Node::new(1, "child"));

        assert!(child.link_parent(parent.clone()).await);
        assert!(!child.link_parent(parent.clone()).await);
    }

    #[tokio::test]
    async fn test_unlink_child_should_break_link_between_linked_nodes() {
        let parent = Arc::new(Node::new(0, "parent"));
        let child = Arc::new(Node::new(1, "child"));

        assert!(parent.link_child(child.clone()).await);

        assert!(parent.unlink_child(child.clone()).await);

        assert!(!parent.has_children().await);
        assert!(!child.has_parents().await);
    }

    #[tokio::test]
    async fn test_second_unlink_child_should_return_false() {
        let parent = Arc::new(Node::new(0, "parent"));
        let child = Arc::new(Node::new(1, "child"));

        assert!(parent.link_child(child.clone()).await);

        assert!(parent.unlink_child(child.clone()).await);
        assert!(!parent.unlink_child(child.clone()).await);
    }

    #[tokio::test]
    async fn test_unlink_parent_should_break_link_between_linked_nodes() {
        let parent = Arc::new(Node::new(0, "parent"));
        let child = Arc::new(Node::new(1, "child"));

        assert!(parent.link_child(child.clone()).await);

        assert!(child.unlink_parent(parent.clone()).await);

        assert!(!parent.has_children().await);
        assert!(!child.has_parents().await);
    }

    #[tokio::test]
    async fn test_second_unlink_parent_should_return_false() {
        let parent = Arc::new(Node::new(0, "parent"));
        let child = Arc::new(Node::new(1, "child"));

        assert!(child.link_parent(parent.clone()).await);

        assert!(child.unlink_parent(parent.clone()).await);
        assert!(!child.unlink_parent(parent.clone()).await);
    }

    #[tokio::test]
    async fn test_child_ids_should_return_children_identifiers_in_vector() {
        let parent = Arc::new(Node::new(0, "parent"));

        let child1 = Arc::new(Node::new(1, "child1"));
        let child2 = Arc::new(Node::new(2, "child2"));

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
        let parent1 = Arc::new(Node::new(0, "parent"));
        let parent2 = Arc::new(Node::new(1, "parent"));

        let child = Arc::new(Node::new(2, "child1"));

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
        let root_node = Arc::new(Node::new(0, "root"));
        let child_node = Arc::new(Node::new(1, "child"));

        assert!(child_node.link_parent(root_node.clone()).await);

        assert!(!root_node.has_parents().await);
        assert!(root_node.has_children().await);
        assert!(child_node.has_parents().await);
        assert!(!child_node.has_children().await);
    }

    #[tokio::test]
    async fn test_has_child_should_return_true_with_correct_child_id() {
        let node0 = Arc::new(Node::new(0, "root"));
        let node1 = Arc::new(Node::new(1, "child"));

        node0.link_child(node1.clone()).await;

        assert!(node0.has_child(1).await);
    }

    #[tokio::test]
    async fn test_has_child_should_return_false_with_incorrect_child_id() {
        let node = Node::new(0, "test");

        assert!(!node.has_child(1).await);
    }

    #[tokio::test]
    async fn test_has_parent_should_return_true_with_correct_parent_id() {
        let node0 = Arc::new(Node::new(0, "n0"));
        let node1 = Arc::new(Node::new(1, "n1"));

        node1.link_parent(node0.clone()).await;

        assert!(node1.has_parent(0).await);
    }

    #[tokio::test]
    async fn test_has_parent_should_return_false_with_incorrect_parent_id() {
        let node = Node::new(0, "test");

        assert!(!node.has_parent(1).await);
    }

    #[tokio::test]
    async fn test_allow_link_self_node_as_child() {
        let node = Arc::new(Node::new(0, "test"));

        node.link_child(node.clone()).await;

        assert!(node.has_child(node.id()).await);
    }
}
