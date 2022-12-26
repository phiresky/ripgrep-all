use std::pin::Pin;

use tokio_stream::Stream;

use crate::adapters::AdaptInfo;

pub trait AdaptedFilesIter: Stream<Item = anyhow::Result<AdaptInfo>> + Send {}
impl<T> AdaptedFilesIter for T where T: Stream<Item = anyhow::Result<AdaptInfo>> + Send {}

pub type AdaptedFilesIterBox = Pin<Box<dyn AdaptedFilesIter>>;

pub fn one_file(ai: AdaptInfo) -> AdaptedFilesIterBox {
    Box::pin(tokio_stream::once(Ok(ai)))
}
