use async_trait::async_trait;
use tokio::sync::broadcast;

use crate::{Error as CGError, default_handler::DefaultHandler};

#[async_trait]
pub trait LinksProvider<S, I>: Send {
    async fn provide_receiver(
        &mut self,
        src_idx: usize,
    ) -> Result<Option<broadcast::Receiver<S>>, CGError<I>>;

    fn try_provide_receiver(
        &mut self,
        src_idx: usize,
    ) -> Result<Option<broadcast::Receiver<S>>, CGError<I>>;
}

#[async_trait]
impl<S, I> LinksProvider<S, I> for DefaultHandler {
    async fn provide_receiver(
        &mut self,
        _: usize,
    ) -> Result<Option<broadcast::Receiver<S>>, CGError<I>> {
        Ok(None)
    }

    fn try_provide_receiver(
        &mut self,
        _: usize,
    ) -> Result<Option<broadcast::Receiver<S>>, CGError<I>> {
        Ok(None)
    }
}
