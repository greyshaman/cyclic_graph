use std::hash::Hash;
use std::{fmt::Debug, sync::Arc};

use crate::content::{layer_content::LayerContent, simple_content::SimpleContent};

/// Content represents data that is contained in a node.
/// Two types of content can be used: Simple and Layer.
/// Simple is just a container for data,
/// Layer is a container that can contain collection of objects
/// which can connect with each other and transfer signals.
///
/// The generic type parameter I represents ID type that is used
/// for addressing collection objects in the layer.
///
/// The generic type parameter D represents data type that is stored
/// in the Content::Simple case. In case of Layer, the data can't be
/// accessed and has unit () type.
///
/// The generic type parameter S represents signal type that is transferred
/// between the nodes in the layer. In case of Simple, signals are not supported
/// and type is unit ().
#[derive(Debug)]
pub enum Content<I, D = (), S = ()>
where
    I: 'static + Send + Sync + Debug + Clone + Hash + Eq,
    D: 'static + Send + Sync + Debug + Clone,
    S: 'static + Send + Sync + Debug,
{
    Simple(SimpleContent<D>),
    Layer(Arc<dyn LayerContent<IdType = I, PayloadType = D, SignalType = S>>),
}

impl<I, D, S> Content<I, D, S>
where
    I: 'static + Send + Sync + Debug + Clone + Hash + Eq,
    D: 'static + Send + Sync + Debug + Clone,
    S: 'static + Send + Sync + Debug,
{
    /// Creates new instance of Content::Simple.
    pub fn new_simple(data: D) -> Self {
        Content::Simple(SimpleContent::new(data))
    }

    /// Creates new instance of Content::Layer.
    pub fn new_layer(
        layer: Arc<dyn LayerContent<IdType = I, PayloadType = D, SignalType = S>>,
    ) -> Self {
        Content::Layer(layer)
    }

    /// Returns the pointer to the layer content.
    /// Returns None in case of Content::Simple.
    pub fn as_layer(
        &self,
    ) -> Option<Arc<dyn LayerContent<IdType = I, PayloadType = D, SignalType = S>>> {
        match self {
            // In Simple case there is no layer.
            Content::Simple(_) => None,
            // In Layer case the pointer to the layer is returned.
            Content::Layer(layer) => Some(layer.clone()),
        }
    }

    /// Returns the simple data content. In Layer case returns None.
    pub fn as_simple(&self) -> Option<SimpleContent<D>> {
        match self {
            // In Simple case the data can be returned.
            Content::Simple(content) => Some(content.clone()),
            // In Layer case nothing can be returned.
            Content::Layer(_) => None,
        }
    }

    /// Returns true if Content is Layer.
    pub fn is_layer(&self) -> bool {
        match self {
            // In Simple case the result is false.
            Content::Simple(_) => false,
            // In Layer case the result is true.
            Content::Layer(_) => true,
        }
    }

    /// Returns true if Content is Simple.
    pub fn is_simple(&self) -> bool {
        !self.is_layer()
    }
}
