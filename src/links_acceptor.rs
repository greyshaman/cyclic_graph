use async_trait::async_trait;

use crate::{Error as CGError, default_handler::DefaultHandler, links_provider::LinksProvider};

#[async_trait]
pub trait LinksAcceptor<S, I>: Send {
    async fn connect<LP: LinksProvider<S, I>>(&self, provider: &LP) -> Result<bool, CGError<I>>;

    fn try_connect<LP: LinksProvider<S, I>>(&self, provider: &LP) -> Result<bool, CGError<I>>;
}

#[async_trait]
impl<S, I> LinksAcceptor<S, I> for DefaultHandler {
    async fn connect<LP: LinksProvider<S, I>>(&self, _: &LP) -> Result<bool, CGError<I>> {
        Ok(true)
    }

    fn try_connect<LP: LinksProvider<S, I>>(&self, _: &LP) -> Result<bool, CGError<I>> {
        Ok(true)
    }
}
