use std::{
    borrow::Borrow, collections::HashSet, fmt::Debug, hash::Hash, marker::PhantomData, sync::Arc,
};

use tokio::sync::RwLock;

use crate::{content::content::Content, error::CyclicGraphError as CGError};

/// A node in a graph with a set of ancestor and descendant nodes, a unique identifier,
/// and a payload that it is associated with.
/// I - the node identifier type
/// D - the type of node's simple content payload data
/// S - the type of signal which can operate by inner struct in Node.content for Content::Layer
#[derive(Debug)]
pub struct Node<I, D, S = ()>
where
    I: 'static + Send + Sync + Clone + Eq + PartialEq + Hash + Debug,
    D: 'static + Send + Sync + Clone + Debug,
    S: 'static + Send + Sync + Debug,
{
    /// The unique identifier
    id: I,

    /// The payload
    content: Content<I, D, S>,

    /// The parent ids set
    parent_ids: RwLock<HashSet<I>>,

    /// The child ids set
    child_ids: RwLock<HashSet<I>>,

    /// The DataType marker
    _marker: PhantomData<D>,
}

impl<I, D, S> Node<I, D, S>
where
    I: 'static + Send + Sync + Clone + Eq + PartialEq + Hash + Debug,
    D: 'static + Send + Sync + Clone + Debug,
    S: 'static + Send + Sync + Debug,
{
    /// The node constructor
    ///
    /// # Example
    ///
    /// ```
    /// use cyclic_graph::{Node, Content};
    ///
    /// let content = Content::new_simple("one".to_string());
    /// let node_i32 = Node::<i32, String>::new(1, content);
    ///
    /// let content = Content::new_simple("zero".to_string());
    /// let node_usize = Node::<usize, String>::new(0, content);
    ///
    /// let content = Content::new_simple(vec![0_usize, 1, 2]);
    /// let node_string_id = Node::<String, Vec<usize>>::new(
    ///     String::from("HL_0"),
    ///     content,
    /// );
    /// ```
    pub fn new(id: I, content: Content<I, D, S>) -> Self {
        Self {
            id,
            content,
            parent_ids: RwLock::new(HashSet::new()),
            child_ids: RwLock::new(HashSet::new()),
            _marker: PhantomData,
        }
    }

    /// Returns id
    pub fn id(&self) -> &I {
        &self.id
    }

    /// Returns the node's content.
    pub fn content(&self) -> &Content<I, D, S> {
        &self.content
    }

    /// Returns wrapped payload value of the node's content.
    /// Returns None if Node's content is not of type Content::Simple.
    pub async fn value(&self) -> Option<D> {
        match &self.content {
            Content::Simple(data) => Some(data.value().await),
            Content::Layer(_) => None,
        }
    }

    /// Sets the new value of the node's content.
    ///
    /// # Arguments
    ///
    /// * `new_data` - The new data value to be set.
    ///
    /// # Returns
    ///
    /// Returns `Some(old_data)` if the data value was successfully set, where `old_data` is the previous data value.
    /// Returns `None` if the data value was not set or Node's content is not of type Content::Simple.
    pub async fn set_value(&self, new_value: D) -> Option<D> {
        match &self.content {
            Content::Simple(data) => data.set_value(new_value).await,
            Content::Layer(_) => None,
        }
    }

    async fn link_nodes(
        &self,
        other: Arc<Node<I, D, S>>,
        self_ids: &RwLock<HashSet<I>>,
        other_ids: &RwLock<HashSet<I>>,
        is_parent_to_child: bool,
    ) -> Result<bool, CGError<I>> {
        if self.id == other.id {
            return Err(CGError::CannotLinkToItself);
        }
        let result = self_ids.write().await.insert(other.id().clone())
            && other_ids.write().await.insert(self.id().clone());
        // Check if the link was successfully established and self.content is Content::Layer
        // and other.content is Content::Layer then call link_accept handler of the content.
        if result {
            if is_parent_to_child {
                // When self - parent, other - child
                if let Some(other_layer) = other.content.as_layer() {
                    return other_layer.connect(&self.content).await;
                }
            } else {
                // When self - child, other - parent
                if let Some(self_layer) = self.content.as_layer() {
                    return self_layer.connect(&other.content).await;
                }
            }
        }

        Ok(result)
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

    /// Creates link to specified child and from child to current node as to parent
    pub async fn link_child(&self, child: Arc<Node<I, D, S>>) -> Result<bool, CGError<I>> {
        self.link_nodes(child.clone(), &self.child_ids, &child.parent_ids, true)
            .await
    }

    /// Creates link to specified child and from child to current node as to parent synchronously
    pub fn try_link_child(&self, child: Arc<Node<I, D, S>>) -> Result<bool, CGError<I>> {
        if self.id == child.id {
            return Err(CGError::CannotLinkToItself);
        }
        self.child_ids
            .try_write()
            .map(|mut src_child_ids| src_child_ids.insert(child.id().clone()))
            .and_then(|insert_result| {
                if !insert_result {
                    return Ok(insert_result);
                }
                child
                    .parent_ids
                    .try_write()
                    .map(|mut parent_ids| parent_ids.insert(self.id().clone()))
            })
            .map_err(CGError::from)
            .and_then(|insert_result| {
                if !insert_result {
                    return Ok(insert_result);
                }
                child
                    .content
                    .as_layer()
                    .map(|layer| layer.try_connect(&self.content))
                    .unwrap_or(Ok(insert_result))
            })
    }

    /// Removes links between child and current node as parent
    pub async fn unlink_child(&self, child: Arc<Node<I, D, S>>) -> Result<bool, CGError<I>> {
        let res = if let Some(layer) = child.content.as_layer() {
            layer.disconnect(&self.content).await?
        } else {
            true
        };

        Ok(res
            && self.child_ids.write().await.remove(child.id())
            && child.parent_ids.write().await.remove(self.id()))
    }

    /// Creates link to specified parent and from parent to current node as to child
    pub async fn link_parent(&self, parent: Arc<Node<I, D, S>>) -> Result<bool, CGError<I>> {
        self.link_nodes(parent.clone(), &self.parent_ids, &parent.child_ids, false)
            .await
    }

    /// Creates link to specified parent and from parent to current node as to child synchronously
    pub fn try_link_parent(&self, parent: Arc<Node<I, D, S>>) -> Result<bool, CGError<I>> {
        if self.id == parent.id {
            return Err(CGError::CannotLinkToItself);
        }
        self.parent_ids
            .try_write()
            .map(|mut src_parent_ids| src_parent_ids.insert(parent.id().clone()))
            .and_then(|insert_result| {
                if !insert_result {
                    return Ok(insert_result);
                }
                parent
                    .child_ids
                    .try_write()
                    .map(|mut child_ids| child_ids.insert(self.id().clone()))
            })
            .map_err(CGError::from)
            .and_then(|insert_result| {
                if !insert_result {
                    return Ok(insert_result);
                }
                self.content
                    .as_layer()
                    .map(|layer| layer.try_connect(&parent.content))
                    .unwrap_or(Ok(insert_result))
            })
    }

    /// Removes links between parent and current node as child
    pub async fn unlink_parent(&self, parent: Arc<Node<I, D, S>>) -> Result<bool, CGError<I>> {
        // let res = self.content.disconnect(&parent.content).await?;
        let res = if let Some(layer) = self.content.as_layer() {
            layer.disconnect(&parent.content).await?
        } else {
            true
        };

        Ok(res
            && self.parent_ids.write().await.remove(parent.id())
            && parent.child_ids.write().await.remove(self.id()))
    }

    /// Checks if current node has parent node specified by id
    pub async fn has_parent<Q>(&self, id: &Q) -> bool
    where
        I: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        self.parent_ids.read().await.contains(id)
    }

    /// Checks if current node has connections to data
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

    fn create_simple_node<I, D>(id: I, content_data: D) -> Arc<Node<I, D, ()>>
    where
        I: Hash + Eq + Clone + Copy + Send + Sync + Debug,
        D: Hash + Eq + Clone + Send + Sync + Debug,
    {
        Arc::new(Node::<I, D>::new(id, Content::new_simple(content_data)))
    }

    mod for_id_as_usize {
        use std::error::Error;

        use super::*;

        #[tokio::test]
        async fn test_simple_create_new_node() {
            let node =
                Node::<usize, String, ()>::new(0_usize, Content::new_simple("test".to_string()));

            assert_eq!(node.id, 0);
        }

        #[tokio::test]
        async fn test_create_new_node() {
            let node = create_simple_node(0_usize, "test".to_string());

            let vec_node = create_simple_node(1_usize, vec![1_usize, 2, 3]);

            assert_eq!(node.id, 0);
            assert_eq!(node.value().await.unwrap(), "test");
            assert!(node.parent_ids.read().await.is_empty());
            assert!(node.child_ids.read().await.is_empty());

            assert_eq!(vec_node.id, 1);
            assert_eq!(vec_node.value().await.unwrap().len(), 3);
        }

        #[test]
        fn test_id_accessor_should_return_correct_id_value() {
            let node = create_simple_node(0_usize, "test".to_string());

            assert_eq!(node.id(), &0);
        }

        #[tokio::test]
        async fn test_data_value_accessor_should_return_correct_data_ref() {
            let node = create_simple_node(0_usize, "test".to_string());

            assert_eq!(node.value().await.unwrap(), "test");
        }

        #[tokio::test]
        async fn test_set_data_value_allowed_to_change_node_data() -> Result<(), Box<dyn Error>> {
            let node = create_simple_node(0_usize, "test".to_string());

            let old_data = node.set_value("new test".into()).await;

            assert_eq!(old_data.unwrap(), "test");
            assert_eq!(node.value().await.unwrap(), "new test");

            Ok(())
        }

        #[tokio::test]
        async fn test_linking_two_nodes_by_link_child() -> Result<(), Box<dyn Error>> {
            let root_node = create_simple_node(0_usize, "root".to_string());
            let child_node = create_simple_node(1_usize, "child".to_string());

            let res = root_node.link_child(child_node.clone()).await?;
            assert!(res);

            assert!(!root_node.has_parents().await);
            assert!(root_node.has_children().await);
            assert!(child_node.has_parents().await);
            assert!(!child_node.has_children().await);

            Ok(())
        }

        #[tokio::test]
        async fn test_linking_two_nodes_by_try_link_child() {
            let root_node = create_simple_node(0_usize, "root".to_string());
            let child_node = create_simple_node(1_usize, "child".to_string());

            let op_res = root_node.try_link_child(child_node.clone());
            assert!(op_res.is_ok());
            assert!(op_res.unwrap());

            assert!(!root_node.has_parents().await);
            assert!(root_node.has_children().await);
            assert!(child_node.has_parents().await);
            assert!(!child_node.has_children().await);
        }

        #[tokio::test]
        async fn test_second_link_child_attempt_should_return_false() -> Result<(), Box<dyn Error>>
        {
            let parent = create_simple_node(0_usize, "parent".to_string());
            let child = create_simple_node(1_usize, "child".to_string());

            assert!(parent.link_child(child.clone()).await?);
            assert!(!parent.link_child(child.clone()).await?);

            Ok(())
        }

        #[test]
        fn test_second_try_link_child_attempt_should_return_false() -> Result<(), Box<dyn Error>> {
            let parent = create_simple_node(0_usize, "parent".to_string());
            let child = create_simple_node(1_usize, "child".to_string());

            assert!(parent.try_link_child(child.clone())?);
            assert!(!parent.try_link_child(child.clone())?);

            Ok(())
        }

        #[tokio::test]
        async fn test_second_link_parent_attempt_should_return_false() -> Result<(), Box<dyn Error>>
        {
            let parent = create_simple_node(0_usize, "parent".to_string());
            let child = create_simple_node(1_usize, "child".to_string());

            assert!(child.link_parent(parent.clone()).await?);
            assert!(!child.link_parent(parent.clone()).await?);

            Ok(())
        }

        #[tokio::test]
        async fn test_unlink_child_should_break_link_between_linked_nodes()
        -> Result<(), Box<dyn Error>> {
            let parent = create_simple_node(0_usize, "parent".to_string());
            let child = create_simple_node(1_usize, "child".to_string());

            assert!(parent.link_child(child.clone()).await?);

            assert!(parent.unlink_child(child.clone()).await?);

            assert!(!parent.has_children().await);
            assert!(!child.has_parents().await);

            Ok(())
        }

        #[tokio::test]
        async fn test_second_unlink_child_should_return_false() -> Result<(), Box<dyn Error>> {
            let parent = create_simple_node(0_usize, "parent".to_string());
            let child = create_simple_node(1_usize, "child".to_string());

            assert!(parent.link_child(child.clone()).await?);

            assert!(parent.unlink_child(child.clone()).await?);
            assert!(!parent.unlink_child(child.clone()).await?);

            Ok(())
        }

        #[tokio::test]
        async fn test_unlink_parent_should_break_link_between_linked_nodes()
        -> Result<(), Box<dyn Error>> {
            let parent = create_simple_node(0_usize, "parent".to_string());
            let child = create_simple_node(1_usize, "child".to_string());

            assert!(parent.link_child(child.clone()).await?);

            assert!(child.unlink_parent(parent.clone()).await?);

            assert!(!parent.has_children().await);
            assert!(!child.has_parents().await);

            Ok(())
        }

        #[tokio::test]
        async fn test_second_unlink_parent_should_return_false() -> Result<(), Box<dyn Error>> {
            let parent = create_simple_node(0_usize, "parent".to_string());
            let child = create_simple_node(1_usize, "child".to_string());

            assert!(child.link_parent(parent.clone()).await?);

            assert!(child.unlink_parent(parent.clone()).await?);
            assert!(!child.unlink_parent(parent.clone()).await?);

            Ok(())
        }

        #[tokio::test]
        async fn test_child_ids_should_return_children_identifiers_in_vector()
        -> Result<(), Box<dyn Error>> {
            let parent = create_simple_node(0_usize, "parent".to_string());
            let child1 = create_simple_node(1_usize, "child1".to_string());
            let child2 = create_simple_node(2_usize, "child2".to_string());

            assert!(parent.link_child(child1.clone()).await?);
            assert!(parent.link_child(child2.clone()).await?);

            let ids = parent.child_ids().await;

            assert!(!ids.is_empty());
            assert_eq!(ids.len(), 2);
            assert!(ids.contains(&1));
            assert!(ids.contains(&2));

            Ok(())
        }

        #[tokio::test]
        async fn test_children_ids_should_return_children_identifiers_in_vector()
        -> Result<(), Box<dyn Error>> {
            let parent1 = create_simple_node(0_usize, "parent1".to_string());
            let parent2 = create_simple_node(1_usize, "parent2".to_string());
            let child = create_simple_node(2_usize, "child1".to_string());

            assert!(parent1.link_child(child.clone()).await?);
            assert!(parent2.link_child(child.clone()).await?);

            let ids = child.parent_ids().await;

            assert!(!ids.is_empty());
            assert_eq!(ids.len(), 2);
            assert!(ids.contains(&0));
            assert!(ids.contains(&1));

            Ok(())
        }

        #[tokio::test]
        async fn test_linking_two_nodes_by_link_parent() -> Result<(), Box<dyn Error>> {
            let root_node = create_simple_node(0_usize, "root".to_string());
            let child_node = create_simple_node(1_usize, "child".to_string());

            assert!(child_node.link_parent(root_node.clone()).await?);

            assert!(!root_node.has_parents().await);
            assert!(root_node.has_children().await);
            assert!(child_node.has_parents().await);
            assert!(!child_node.has_children().await);

            Ok(())
        }

        #[tokio::test]
        async fn test_has_child_should_return_true_with_correct_child_id()
        -> Result<(), Box<dyn Error>> {
            let node0 = create_simple_node(0_usize, "root".to_string());
            let node1 = create_simple_node(1_usize, "child".to_string());

            assert!(node0.link_child(node1.clone()).await?);

            assert!(node0.has_child(&1).await);

            Ok(())
        }

        #[tokio::test]
        async fn test_has_child_should_return_false_with_incorrect_child_id() {
            let node = create_simple_node(0_usize, "test".to_string());

            assert!(!node.has_child(&1).await);
        }

        #[tokio::test]
        async fn test_has_parent_should_return_true_with_correct_parent_id()
        -> Result<(), Box<dyn Error>> {
            let node0 = create_simple_node(0_usize, "n0".to_string());
            let node1 = create_simple_node(1_usize, "n1".to_string());

            assert!(node1.link_parent(node0.clone()).await?);

            assert!(node1.has_parent(&0).await);

            Ok(())
        }

        #[tokio::test]
        async fn test_has_parent_should_return_false_with_incorrect_parent_id() {
            let node = create_simple_node(0_usize, "test".to_string());

            assert!(!node.has_parent(&1).await);
        }

        #[tokio::test]
        async fn test_not_allow_link_self_node_as_child() {
            let node = create_simple_node(0_usize, "test".to_string());

            assert!(node.link_child(node.clone()).await.is_err());
        }

        #[tokio::test]
        async fn test_allow_cyclic_links_between_two_nodes() -> Result<(), Box<dyn Error>> {
            let node0 = create_simple_node(0_usize, "test0".to_string());
            let node1 = create_simple_node(1_usize, "test1".to_string());

            assert!(node0.link_child(node1.clone()).await?);
            assert!(node1.link_child(node0.clone()).await?);

            assert!(node0.has_child(node1.id()).await);
            assert!(node1.has_child(node0.id()).await);

            assert!(node0.has_parent(node1.id()).await);
            assert!(node1.has_parent(node0.id()).await);

            Ok(())
        }
    }

    mod for_id_as_str {
        use super::*;
        use std::error::Error;

        #[tokio::test]
        async fn test_create_new_node() {
            let node = create_simple_node("IL", "test".to_string());
            let vec_node = create_simple_node("IL", vec![1_usize, 2, 3]);

            assert_eq!(node.id, "IL");
            assert_eq!(node.value().await.unwrap(), "test");
            assert!(node.parent_ids.read().await.is_empty());
            assert!(node.child_ids.read().await.is_empty());

            assert_eq!(vec_node.id, "IL");
            assert_eq!(vec_node.value().await.unwrap().len(), 3);
            assert_eq!(vec_node.value().await.unwrap()[0], 1);
            assert_eq!(vec_node.value().await.unwrap()[1], 2);
            assert_eq!(vec_node.value().await.unwrap()[2], 3);
        }

        #[test]
        fn test_id_accessor_should_return_correct_id_value() {
            let node = create_simple_node("IL", "test".to_string());

            assert_eq!(node.id(), &"IL");
        }

        #[tokio::test]
        async fn test_value_accessor_should_return_correct_data_value() {
            let node = create_simple_node("IL", "test".to_string());

            assert_eq!(node.value().await.unwrap(), "test");
        }

        #[tokio::test]
        async fn test_data_mut_accessor_allowed_to_change_node_data() {
            let node = create_simple_node("IL", "test".to_string());

            {
                let content = node.content().as_simple().unwrap().content();
                let mut data_mut = content.write().await;
                *data_mut = "new test".into();
            }

            assert_eq!(node.value().await.unwrap(), "new test");
        }

        #[tokio::test]
        async fn test_set_value_should_change_node_data() -> Result<(), Box<dyn Error>> {
            let node = create_simple_node("IL", "test".to_string());

            let prev_data = node.set_value("new test".into()).await;

            assert_eq!(prev_data.unwrap(), "test");
            assert_eq!(node.value().await.unwrap(), "new test");

            Ok(())
        }

        #[tokio::test]
        async fn test_linking_two_nodes_by_link_child() -> Result<(), Box<dyn Error>> {
            let root_node = create_simple_node("n0", "root".to_string());
            let child_node = create_simple_node("n1", "child".to_string());

            assert!(root_node.link_child(child_node.clone()).await?);

            assert!(!root_node.has_parents().await);
            assert!(root_node.has_children().await);
            assert!(child_node.has_parents().await);
            assert!(!child_node.has_children().await);

            Ok(())
        }

        #[tokio::test]
        async fn test_second_link_child_attempt_should_return_false() -> Result<(), Box<dyn Error>>
        {
            let parent = create_simple_node("n0", "parent".to_string());
            let child = create_simple_node("n1", "child".to_string());

            assert!(parent.link_child(child.clone()).await?);
            assert!(!parent.link_child(child.clone()).await?);

            Ok(())
        }

        #[tokio::test]
        async fn test_second_link_parent_attempt_should_return_false() -> Result<(), Box<dyn Error>>
        {
            let parent = create_simple_node("n0", "parent".to_string());
            let child = create_simple_node("n1", "child".to_string());

            assert!(child.link_parent(parent.clone()).await?);
            assert!(!child.link_parent(parent.clone()).await?);

            Ok(())
        }

        #[tokio::test]
        async fn test_unlink_child_should_break_link_between_linked_nodes()
        -> Result<(), Box<dyn Error>> {
            let parent = create_simple_node("n0", "parent".to_string());
            let child = create_simple_node("n1", "child".to_string());

            assert!(parent.link_child(child.clone()).await?);

            assert!(parent.unlink_child(child.clone()).await?);

            assert!(!parent.has_children().await);
            assert!(!child.has_parents().await);

            Ok(())
        }

        #[tokio::test]
        async fn test_second_unlink_child_should_return_false() -> Result<(), Box<dyn Error>> {
            let parent = create_simple_node("n0", "parent".to_string());
            let child = create_simple_node("n1", "child".to_string());

            assert!(parent.link_child(child.clone()).await?);

            assert!(parent.unlink_child(child.clone()).await?);
            assert!(!parent.unlink_child(child.clone()).await?);

            assert!(!parent.has_children().await);
            assert!(!child.has_parents().await);

            Ok(())
        }

        #[tokio::test]
        async fn test_unlink_parent_should_break_link_between_linked_nodes()
        -> Result<(), Box<dyn Error>> {
            let parent = create_simple_node("n0", "parent".to_string());
            let child = create_simple_node("n1", "child".to_string());

            assert!(parent.link_child(child.clone()).await?);

            assert!(child.unlink_parent(parent.clone()).await?);

            assert!(!parent.has_children().await);
            assert!(!child.has_parents().await);

            Ok(())
        }

        #[tokio::test]
        async fn test_second_unlink_parent_should_return_false() -> Result<(), Box<dyn Error>> {
            let parent = create_simple_node("n0", "parent".to_string());
            let child = create_simple_node("n1", "child".to_string());

            assert!(child.link_parent(parent.clone()).await?);

            assert!(child.unlink_parent(parent.clone()).await?);
            assert!(!child.unlink_parent(parent.clone()).await?);

            Ok(())
        }

        #[tokio::test]
        async fn test_child_ids_should_return_children_identifiers_in_vector()
        -> Result<(), Box<dyn Error>> {
            let parent = create_simple_node("n0", "parent".to_string());
            let child1 = create_simple_node("n1", "child1".to_string());
            let child2 = create_simple_node("n2", "child2".to_string());

            assert!(parent.link_child(child1.clone()).await?);
            assert!(parent.link_child(child2.clone()).await?);

            let ids = parent.child_ids().await;

            assert!(!ids.is_empty());
            assert_eq!(ids.len(), 2);
            assert!(ids.contains(&"n1"));
            assert!(ids.contains(&"n2"));

            Ok(())
        }

        #[tokio::test]
        async fn test_children_ids_should_return_children_identifiers_in_vector()
        -> Result<(), Box<dyn Error>> {
            let parent1 = create_simple_node("n0", "parent1".to_string());
            let parent2 = create_simple_node("n1", "parent2".to_string());
            let child = create_simple_node("n2", "child".to_string());

            assert!(parent1.link_child(child.clone()).await?);
            assert!(parent2.link_child(child.clone()).await?);

            let ids = child.parent_ids().await;

            assert!(!ids.is_empty());
            assert_eq!(ids.len(), 2);
            assert!(ids.contains(&"n0"));
            assert!(ids.contains(&"n1"));

            Ok(())
        }

        #[tokio::test]
        async fn test_linking_two_nodes_by_link_parent() -> Result<(), Box<dyn Error>> {
            let root_node = create_simple_node("n0", "root".to_string());
            let child_node = create_simple_node("n1", "child".to_string());

            assert!(child_node.link_parent(root_node.clone()).await?);

            assert!(!root_node.has_parents().await);
            assert!(root_node.has_children().await);
            assert!(child_node.has_parents().await);
            assert!(!child_node.has_children().await);

            Ok(())
        }

        #[tokio::test]
        async fn test_has_child_should_return_true_with_correct_child_id()
        -> Result<(), Box<dyn Error>> {
            let node0 = create_simple_node("n0", "root".to_string());
            let node1 = create_simple_node("n1", "child".to_string());

            assert!(node0.link_child(node1.clone()).await?);

            assert!(node0.has_child(&"n1").await);

            Ok(())
        }

        #[tokio::test]
        async fn test_has_child_should_return_false_with_incorrect_child_id() {
            let node = create_simple_node("n0", "test".to_string());

            assert!(!node.has_child(&"n1").await);
        }

        #[tokio::test]
        async fn test_has_parent_should_return_true_with_correct_parent_id()
        -> Result<(), Box<dyn Error>> {
            let node0 = create_simple_node("n0", "n0".to_string());
            let node1 = create_simple_node("n1", "n1".to_string());

            assert!(node1.link_parent(node0.clone()).await?);

            assert!(node1.has_parent(&"n0").await);

            Ok(())
        }

        #[tokio::test]
        async fn test_has_parent_should_return_false_with_incorrect_parent_id() {
            let node = create_simple_node("n0", "test".to_string());

            assert!(!node.has_parent(&"n1").await);
        }

        #[tokio::test]
        async fn test_not_allow_link_self_node_as_child() {
            let node = create_simple_node("n0", "test".to_string());

            assert!(node.link_child(node.clone()).await.is_err());
        }

        #[tokio::test]
        async fn test_allow_cyclic_links_between_two_nodes() -> Result<(), Box<dyn Error>> {
            let node0 = create_simple_node("n0", "test0".to_string());
            let node1 = create_simple_node("n1", "test1".to_string());

            assert!(node0.link_child(node1.clone()).await?);
            assert!(node1.link_child(node0.clone()).await?);

            assert!(node0.has_child(node1.id()).await);
            assert!(node1.has_child(node0.id()).await);

            assert!(node0.has_parent(node1.id()).await);
            assert!(node1.has_parent(node0.id()).await);

            Ok(())
        }
    }

    mod for_struct_with_inner_connected_cells {
        use super::*;

        use std::{
            any::Any,
            collections::{HashMap, hash_map::Entry},
            error::Error,
            sync::Arc,
        };

        use async_trait::async_trait;
        use tokio::sync::{RwLock, broadcast};

        use crate::{Content, Error as CGError, content::layer_content::LayerContent};

        /// The Cell
        #[derive(Debug)]
        struct InnerCell {
            inputs: HashMap<usize, broadcast::Receiver<u8>>,
            output: broadcast::Sender<u8>,
        }

        impl InnerCell {
            fn new(capacity: usize) -> Self {
                let (tx, _) = broadcast::channel::<u8>(capacity);
                Self {
                    inputs: HashMap::new(),
                    output: tx,
                }
            }
        }

        /// Like Layer
        type Cellular = HashMap<usize, InnerCell>;

        const CELLULAR_CAPACITY: usize = 10;

        /// Content for cells layer
        #[derive(Debug)]
        struct CellularLayer {
            data: Arc<RwLock<Cellular>>,
        }

        impl CellularLayer {
            fn new(size: usize, node_id: usize) -> Self {
                if size > CELLULAR_CAPACITY {
                    panic!("The size should be less then {}", CELLULAR_CAPACITY)
                };
                let cell_group_id_prefix = node_id * CELLULAR_CAPACITY;
                let mut cellulars = HashMap::with_capacity(size);
                for id in 0..size {
                    cellulars.insert(cell_group_id_prefix + id, InnerCell::new(2));
                }
                Self {
                    data: Arc::new(RwLock::new(cellulars)),
                }
            }

            async fn has_incoming_channels(&self) -> bool {
                self.data
                    .read()
                    .await
                    .values()
                    .any(|item| item.inputs.len() > 0)
            }

            async fn count_incoming_channels(&self) -> usize {
                self.data
                    .read()
                    .await
                    .values()
                    .fold(0_usize, |acc, item| acc + item.inputs.len())
            }

            async fn count_outgoing_channels(&self) -> usize {
                self.data
                    .read()
                    .await
                    .values()
                    .fold(0_usize, |acc, item| acc + item.output.receiver_count())
            }
        }

        #[async_trait]
        impl LayerContent for CellularLayer {
            type IdType = usize;
            type PayloadType = ();
            type SignalType = u8;

            fn as_any(&self) -> &dyn Any {
                self
            }

            async fn provide_receiver(
                &self,
                src_idx: usize,
            ) -> Result<broadcast::Receiver<u8>, CGError<usize>> {
                match self.data.write().await.entry(src_idx) {
                    Entry::Occupied(entry) => Ok(entry.get().output.subscribe()),
                    Entry::Vacant(_) => Err(CGError::LinksProviderHandlerError(format!(
                        "Cell with id {} not found at provider content",
                        src_idx
                    ))),
                }
            }

            fn try_provide_receiver(
                &self,
                src_idx: usize,
            ) -> Result<broadcast::Receiver<u8>, CGError<usize>> {
                match self.data.try_write()?.entry(src_idx) {
                    Entry::Occupied(entry) => Ok(entry.get().output.subscribe()),
                    Entry::Vacant(_) => Err(CGError::LinksProviderHandlerError(format!(
                        "Cell with id {} not found at provider content",
                        src_idx
                    ))),
                }
            }

            async fn provide_src_ids(&self) -> Vec<usize> {
                self.data
                    .read()
                    .await
                    .keys()
                    .cloned()
                    .collect::<Vec<usize>>()
            }

            fn try_provide_src_ids(&self) -> Result<Vec<usize>, CGError<usize>> {
                let ids = self
                    .data
                    .try_read()?
                    .keys()
                    .cloned()
                    .collect::<Vec<usize>>();
                Ok(ids)
            }

            async fn connect(
                &self,
                link_source_content: &Content<usize, (), u8>,
            ) -> Result<bool, CGError<usize>> {
                let mut result = true;
                if let Some(layer) = link_source_content.as_layer() {
                    let src_ids = layer.provide_src_ids().await;
                    let mut w_dst_data = self.data.write().await;
                    for dst_cell in w_dst_data.values_mut() {
                        for src_idx in &src_ids {
                            let rx = layer.provide_receiver(src_idx.clone()).await?;
                            if let Entry::Vacant(dst_entry) = dst_cell.inputs.entry(*src_idx) {
                                dst_entry.insert(rx);
                                result &= true;
                            } else {
                                result &= false;
                            }
                        }
                    }
                }
                Ok(result)
            }

            fn try_connect(
                &self,
                link_source_content: &Content<usize, (), u8>,
            ) -> Result<bool, CGError<usize>> {
                let mut result = true;
                if let Some(layer) = link_source_content.as_layer() {
                    let src_ids = layer.try_provide_src_ids()?;
                    for dst_cell in self.data.try_write()?.values_mut() {
                        for src_idx in &src_ids {
                            let rx = layer.try_provide_receiver(src_idx.clone())?;
                            if let Entry::Vacant(dst_entry) = dst_cell.inputs.entry(*src_idx) {
                                dst_entry.insert(rx);
                                result &= true;
                            } else {
                                result &= false;
                            }
                        }
                    }
                }
                Ok(result)
            }

            async fn disconnect(
                &self,
                link_source_content: &Content<usize, (), u8>,
            ) -> Result<bool, CGError<usize>> {
                let mut result = true;
                if let Some(layer) = link_source_content.as_layer() {
                    let src_ids = layer.provide_src_ids().await;
                    let mut w_dst_data = self.data.write().await;
                    for dst_cell in w_dst_data.values_mut() {
                        if !dst_cell.inputs.is_empty() {
                            for src_idx in &src_ids {
                                match dst_cell.inputs.entry(*src_idx) {
                                    Entry::Occupied(dst_entry) => {
                                        dst_entry.remove();
                                        result &= true;
                                    }
                                    Entry::Vacant(_) => result &= false,
                                }
                            }
                        }
                    }
                }

                Ok(result)
            }

            fn try_disconnect(
                &self,
                link_source_content: &Content<usize, (), u8>,
            ) -> Result<bool, CGError<usize>> {
                let mut result = true;
                if let Some(layer) = link_source_content.as_layer() {
                    let src_ids = layer.try_provide_src_ids()?;
                    for dst_cell in self.data.try_write()?.values_mut() {
                        if !dst_cell.inputs.is_empty() {
                            for src_idx in &src_ids {
                                match dst_cell.inputs.entry(*src_idx) {
                                    Entry::Occupied(dst_entry) => {
                                        dst_entry.remove();
                                        result &= true;
                                    }
                                    Entry::Vacant(_) => result &= false,
                                }
                            }
                        }
                    }
                }

                Ok(result)
            }
        }

        fn create_node_with_layer(id: usize, layer_size: usize) -> Arc<Node<usize, (), u8>> {
            let layer = CellularLayer::new(layer_size, id);
            Arc::new(Node::new(id, Content::new_layer(Arc::new(layer))))
        }

        #[tokio::test]
        async fn test_create_new_node() {
            let node = create_node_with_layer(0, 3);

            assert_eq!(node.id, 0);

            assert_eq!(node.value().await, None);

            assert!(!node.has_parents().await);
            assert!(!node.has_children().await);
        }

        #[test]
        fn test_id_accessor_should_return_correct_id_value() {
            let node = create_node_with_layer(0, 3);

            assert_eq!(node.id(), &0);
        }

        #[tokio::test]
        async fn test_linking_two_nodes_by_link_child() -> Result<(), Box<dyn Error>> {
            let root_node = create_node_with_layer(0, 3);
            let child_node = create_node_with_layer(1, 2);

            let result = root_node.link_child(child_node.clone()).await?;
            assert!(result);

            assert!(!root_node.has_parents().await);
            assert!(root_node.has_children().await);
            assert!(child_node.has_parents().await);
            assert!(!child_node.has_children().await);

            let layer = root_node.content.as_layer().unwrap();
            let concrete_root_content = layer.as_any().downcast_ref::<CellularLayer>().unwrap();

            assert!(!concrete_root_content.has_incoming_channels().await);
            assert_eq!(concrete_root_content.count_incoming_channels().await, 0);

            let layer = child_node.content.as_layer().unwrap();
            let concrete_child_content = layer.as_any().downcast_ref::<CellularLayer>().unwrap();

            assert!(concrete_child_content.has_incoming_channels().await);
            assert_eq!(concrete_child_content.count_incoming_channels().await, 6);

            Ok(())
        }

        #[tokio::test]
        async fn test_linking_two_nodes_by_try_link_child() {
            let root_node = create_node_with_layer(0, 3);
            let child_node = create_node_with_layer(1, 2);

            let operation_result = root_node.try_link_child(child_node.clone());
            assert!(operation_result.is_ok());
            assert!(operation_result.unwrap());

            assert!(!root_node.has_parents().await);
            assert!(root_node.has_children().await);
            assert!(child_node.has_parents().await);
            assert!(!child_node.has_children().await);

            let layer = root_node.content.as_layer().unwrap();
            let root_content_layer = layer.as_any().downcast_ref::<CellularLayer>().unwrap();

            assert!(!root_content_layer.has_incoming_channels().await);
            assert_eq!(root_content_layer.count_incoming_channels().await, 0);

            let layer = child_node.content.as_layer().unwrap();
            let concrete_child_content_layer =
                layer.as_any().downcast_ref::<CellularLayer>().unwrap();

            assert!(concrete_child_content_layer.has_incoming_channels().await);
            assert_eq!(
                concrete_child_content_layer.count_incoming_channels().await,
                6
            );
        }

        #[tokio::test]
        async fn test_second_link_child_attempt_should_return_false() -> Result<(), Box<dyn Error>>
        {
            let parent = create_node_with_layer(0, 3);
            let child = create_node_with_layer(1, 2);

            assert!(parent.link_child(child.clone()).await?);
            assert!(!parent.link_child(child.clone()).await?);

            let layer = child.content.as_layer().unwrap();
            let concrete_child_content_layer =
                layer.as_any().downcast_ref::<CellularLayer>().unwrap();

            assert!(concrete_child_content_layer.has_incoming_channels().await);
            assert_eq!(
                concrete_child_content_layer.count_incoming_channels().await,
                6
            );

            Ok(())
        }

        #[tokio::test]
        async fn test_second_try_link_child_attempt_should_return_false()
        -> Result<(), Box<dyn Error>> {
            let parent = create_node_with_layer(0, 3);
            let child = create_node_with_layer(1, 2);

            assert!(parent.try_link_child(child.clone())?);
            assert!(!parent.try_link_child(child.clone())?);

            let layer = child.content.as_layer().unwrap();
            let concrete_child_content_layer =
                layer.as_any().downcast_ref::<CellularLayer>().unwrap();

            assert!(concrete_child_content_layer.has_incoming_channels().await);
            assert_eq!(
                concrete_child_content_layer.count_incoming_channels().await,
                6
            );

            Ok(())
        }

        #[tokio::test]
        async fn test_second_link_parent_attempt_should_return_false() -> Result<(), Box<dyn Error>>
        {
            let parent = create_node_with_layer(0, 3);
            let child = create_node_with_layer(1, 2);

            assert!(child.link_parent(parent.clone()).await?);
            assert!(!child.link_parent(parent.clone()).await?);

            let layer = child.content.as_layer().unwrap();
            let concrete_child_content_layer =
                layer.as_any().downcast_ref::<CellularLayer>().unwrap();

            assert!(concrete_child_content_layer.has_incoming_channels().await);
            assert_eq!(
                concrete_child_content_layer.count_incoming_channels().await,
                6
            );

            Ok(())
        }

        #[tokio::test]
        async fn test_unlink_child_should_break_link_between_linked_nodes()
        -> Result<(), Box<dyn Error>> {
            let parent = create_node_with_layer(0, 3);
            let child = create_node_with_layer(1, 2);

            assert!(parent.link_child(child.clone()).await?);

            let layer = child.content.as_layer().unwrap();
            let concrete_child_content_layer =
                layer.as_any().downcast_ref::<CellularLayer>().unwrap();

            assert!(concrete_child_content_layer.has_incoming_channels().await);
            assert_eq!(
                concrete_child_content_layer.count_incoming_channels().await,
                6
            );

            assert!(parent.unlink_child(child.clone()).await?);

            assert!(!parent.has_children().await);
            assert!(!child.has_parents().await);

            assert!(!concrete_child_content_layer.has_incoming_channels().await);
            assert_eq!(
                concrete_child_content_layer.count_incoming_channels().await,
                0
            );

            Ok(())
        }

        #[tokio::test]
        async fn test_second_unlink_child_should_return_false() -> Result<(), Box<dyn Error>> {
            let parent = create_node_with_layer(0, 3);
            let child = create_node_with_layer(1, 2);

            assert!(parent.link_child(child.clone()).await?);

            assert!(parent.unlink_child(child.clone()).await?);
            assert!(!parent.unlink_child(child.clone()).await?);

            let layer = child.content.as_layer().unwrap();
            let concrete_child_content_layer =
                layer.as_any().downcast_ref::<CellularLayer>().unwrap();

            assert!(!concrete_child_content_layer.has_incoming_channels().await);
            assert_eq!(
                concrete_child_content_layer.count_incoming_channels().await,
                0
            );

            Ok(())
        }

        #[tokio::test]
        async fn test_unlink_parent_should_break_link_between_linked_nodes()
        -> Result<(), Box<dyn Error>> {
            let parent = create_node_with_layer(0, 3);
            let child = create_node_with_layer(1, 2);

            assert!(parent.link_child(child.clone()).await?);

            assert!(child.unlink_parent(parent.clone()).await?);

            assert!(!parent.has_children().await);
            assert!(!child.has_parents().await);

            let layer = child.content.as_layer().unwrap();
            let concrete_child_content_layer =
                layer.as_any().downcast_ref::<CellularLayer>().unwrap();

            assert!(!concrete_child_content_layer.has_incoming_channels().await);
            assert_eq!(
                concrete_child_content_layer.count_incoming_channels().await,
                0
            );

            Ok(())
        }

        #[tokio::test]
        async fn test_second_unlink_parent_should_return_false() -> Result<(), Box<dyn Error>> {
            let parent = create_node_with_layer(0, 3);
            let child = create_node_with_layer(1, 2);

            assert!(child.link_parent(parent.clone()).await?);

            assert!(child.unlink_parent(parent.clone()).await?);
            assert!(!child.unlink_parent(parent.clone()).await?);

            let layer = child.content.as_layer().unwrap();
            let concrete_child_content_layer =
                layer.as_any().downcast_ref::<CellularLayer>().unwrap();

            assert!(!concrete_child_content_layer.has_incoming_channels().await);
            assert_eq!(
                concrete_child_content_layer.count_incoming_channels().await,
                0
            );

            Ok(())
        }

        #[tokio::test]
        async fn test_child_ids_should_return_children_identifiers_in_vector()
        -> Result<(), Box<dyn Error>> {
            let parent = create_node_with_layer(0, 3);
            let child1 = create_node_with_layer(1, 2);
            let child2 = create_node_with_layer(2, 4);

            assert!(parent.link_child(child1.clone()).await?);
            assert!(parent.link_child(child2.clone()).await?);

            let ids = parent.child_ids().await;

            assert!(!ids.is_empty());
            assert_eq!(ids.len(), 2);
            assert!(ids.contains(&1));
            assert!(ids.contains(&2));

            let layer = parent.content.as_layer().unwrap();
            let concrete_parent_content_layer =
                layer.as_any().downcast_ref::<CellularLayer>().unwrap();

            assert_eq!(
                concrete_parent_content_layer
                    .count_outgoing_channels()
                    .await,
                18
            );

            Ok(())
        }

        #[tokio::test]
        async fn test_children_ids_should_return_children_identifiers_in_vector()
        -> Result<(), Box<dyn Error>> {
            let parent1 = create_node_with_layer(0, 3);
            let parent2 = create_node_with_layer(1, 2);
            let child = create_node_with_layer(2, 4);

            assert!(parent1.link_child(child.clone()).await?);
            assert!(parent2.link_child(child.clone()).await?);

            let ids = child.parent_ids().await;

            assert!(!ids.is_empty());
            assert_eq!(ids.len(), 2);
            assert!(ids.contains(&0));
            assert!(ids.contains(&1));

            let layer = child.content.as_layer().unwrap();
            let concrete_children_content_layer =
                layer.as_any().downcast_ref::<CellularLayer>().unwrap();

            assert_eq!(
                concrete_children_content_layer
                    .count_incoming_channels()
                    .await,
                20
            );

            Ok(())
        }

        #[tokio::test]
        async fn test_linking_two_nodes_by_link_parent() -> Result<(), Box<dyn Error>> {
            let parent = create_node_with_layer(0, 3);
            let child = create_node_with_layer(1, 2);

            assert!(child.link_parent(parent.clone()).await?);

            assert!(!parent.has_parents().await);
            assert!(parent.has_children().await);
            assert!(child.has_parents().await);
            assert!(!child.has_children().await);

            let layer = child.content.as_layer().unwrap();
            let concrete_children_content_layer =
                layer.as_any().downcast_ref::<CellularLayer>().unwrap();

            assert_eq!(
                concrete_children_content_layer
                    .count_incoming_channels()
                    .await,
                6
            );

            Ok(())
        }

        #[tokio::test]
        async fn test_has_child_should_return_true_with_correct_child_id()
        -> Result<(), Box<dyn Error>> {
            let parent = create_node_with_layer(0, 3);
            let child = create_node_with_layer(1, 2);

            assert!(parent.link_child(child.clone()).await?);

            assert!(parent.has_child(&1).await);

            Ok(())
        }

        #[tokio::test]
        async fn test_has_child_should_return_false_with_incorrect_child_id() {
            let node = create_node_with_layer(0, 3);

            assert!(!node.has_child(&1).await);
        }

        #[tokio::test]
        async fn test_has_parent_should_return_true_with_correct_parent_id()
        -> Result<(), Box<dyn Error>> {
            let parent = create_node_with_layer(0, 3);
            let child = create_node_with_layer(1, 2);

            assert!(child.link_parent(parent.clone()).await?);

            assert!(child.has_parent(&0).await);

            Ok(())
        }

        #[tokio::test]
        async fn test_has_parent_should_return_false_with_incorrect_parent_id() {
            let node = create_node_with_layer(0, 3);

            assert!(!node.has_parent(&1).await);
        }

        #[tokio::test]
        async fn test_not_allow_link_self_node_as_child() {
            let node = create_node_with_layer(0, 3);

            assert!(node.link_child(node.clone()).await.is_err());
        }

        #[tokio::test]
        async fn test_allow_cyclic_links_between_two_nodes() -> Result<(), Box<dyn Error>> {
            let node0 = create_node_with_layer(0, 3);
            let node1 = create_node_with_layer(1, 2);

            assert!(node0.link_child(node1.clone()).await?);
            assert!(node1.link_child(node0.clone()).await?);

            assert!(node0.has_child(node1.id()).await);
            assert!(node1.has_child(node0.id()).await);

            assert!(node0.has_parent(node1.id()).await);
            assert!(node1.has_parent(node0.id()).await);

            let layer = node0.content.as_layer().unwrap();
            let concrete_node0_content_layer =
                layer.as_any().downcast_ref::<CellularLayer>().unwrap();

            assert!(concrete_node0_content_layer.has_incoming_channels().await);
            assert_eq!(
                concrete_node0_content_layer.count_incoming_channels().await,
                6
            );

            let layer = node1.content.as_layer().unwrap();
            let concrete_node1_content_layer =
                layer.as_any().downcast_ref::<CellularLayer>().unwrap();

            assert!(concrete_node1_content_layer.has_incoming_channels().await);
            assert_eq!(
                concrete_node1_content_layer.count_incoming_channels().await,
                6
            );

            Ok(())
        }
    }
}
