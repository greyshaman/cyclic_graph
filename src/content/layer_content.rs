use std::{any::Any, fmt::Debug, hash::Hash};

use async_trait::async_trait;
use tokio::sync::broadcast;

use crate::{Error as CGError, content::content::Content};

/// LayerContent is a trait for any content element that can be used in a layer.
/// The Layer is responsible for managing the connections between the content elements.
#[async_trait]
pub trait LayerContent: Sync + Send + Debug {
    /// The identifier type of the content element.
    type IdType: 'static + Send + Sync + Debug + Clone + Eq + Hash;
    type PayloadType: 'static + Send + Sync + Debug + Clone;
    /// The type of the signal that is broadcasted to the content elements.
    type SignalType: 'static + Send + Sync + Debug;

    /// Returns a reference to the underlying Any trait object.
    fn as_any(&self) -> &dyn Any;

    /// Asynchronously provide broadcast channel receiver to make connection between
    /// layer's content element with src_idx.
    async fn provide_receiver(
        &self,
        src_idx: Self::IdType,
    ) -> Result<broadcast::Receiver<Self::SignalType>, CGError<Self::IdType>>;

    /// Synchronously provide broadcast channel receiver to make connection between
    /// layer's content element with src_idx.
    fn try_provide_receiver(
        &self,
        src_idx: Self::IdType,
    ) -> Result<broadcast::Receiver<Self::SignalType>, CGError<Self::IdType>>;

    /// Asynchronously get all the source ids of the collected elements.
    async fn provide_src_ids(&self) -> Vec<Self::IdType> {
        vec![]
    }

    /// Synchronously get all the source ids of the collected elements.
    fn try_provide_src_ids(&self) -> Result<Vec<Self::IdType>, CGError<Self::IdType>> {
        Ok(vec![])
    }

    /// Asynchronously connect the collection's elements with the elements from source node.
    async fn connect(
        &self,
        link_source_content: &Content<Self::IdType, Self::PayloadType, Self::SignalType>,
    ) -> Result<bool, CGError<Self::IdType>>;

    /// Synchronously connect the collection's elements with the elements from source node.
    fn try_connect(
        &self,
        link_source_content: &Content<Self::IdType, Self::PayloadType, Self::SignalType>,
    ) -> Result<bool, CGError<Self::IdType>>;

    /// Asynchronously disconnect the collection's elements with the elements from source node.
    async fn disconnect(
        &self,
        link_source_content: &Content<Self::IdType, Self::PayloadType, Self::SignalType>,
    ) -> Result<bool, CGError<Self::IdType>>;

    /// Synchronously disconnect the collection's elements with the elements from source node.
    fn try_disconnect(
        &self,
        link_source_content: &Content<Self::IdType, Self::PayloadType, Self::SignalType>,
    ) -> Result<bool, CGError<Self::IdType>>;
}
