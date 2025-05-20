use std::{any::Any, fmt::Debug, sync::Arc};

use crate::Error as CGError;
use async_trait::async_trait;
use tokio::sync::{RwLock, broadcast};

#[async_trait]
pub trait Content<I: 'static, D: 'static, S: 'static>: Debug + Send + Sync + Any {
    fn as_any(&self) -> &dyn Any;

    fn data(&self) -> Arc<RwLock<D>>;

    fn set_data(&mut self, data: Arc<RwLock<D>>) -> Result<Arc<RwLock<D>>, CGError<I>>;

    async fn provide_receiver(
        &self,
        _src_idx: usize,
    ) -> Result<Option<broadcast::Receiver<S>>, CGError<I>> {
        Ok(None)
    }

    fn try_provide_receiver(
        &self,
        _src_idx: usize,
    ) -> Result<Option<broadcast::Receiver<S>>, CGError<I>> {
        Ok(None)
    }

    async fn provide_src_ids(&self) -> Vec<usize> {
        vec![]
    }

    fn try_provide_src_ids(&self) -> Result<Vec<usize>, CGError<I>> {
        Ok(vec![])
    }

    async fn link_accept(
        &self,
        _provider: Arc<RwLock<dyn Content<I, D, S> + Send + Sync>>,
    ) -> Result<bool, CGError<I>> {
        Ok(true)
    }

    fn try_link_accept(
        &self,
        _provider: Arc<RwLock<dyn Content<I, D, S> + Send + Sync>>,
    ) -> Result<bool, CGError<I>> {
        Ok(true)
    }

    async fn link_disconnect(
        &self,
        _provider: Arc<RwLock<dyn Content<I, D, S> + Send + Sync>>,
    ) -> Result<bool, CGError<I>> {
        Ok(true)
    }

    fn try_link_disconnect(
        &self,
        _provider: Arc<RwLock<dyn Content<I, D, S> + Send + Sync>>,
    ) -> Result<bool, CGError<I>> {
        Ok(true)
    }
}
