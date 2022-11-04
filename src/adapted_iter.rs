use std::pin::Pin;

use tokio_stream::Stream;

use crate::adapters::AdaptInfo;

pub trait AdaptedFilesIter: Stream<Item = AdaptInfo> + Send {}
impl<T> AdaptedFilesIter for T where T: Stream<Item = AdaptInfo> + Send {}

pub type AdaptedFilesIterBox = Pin<Box<dyn AdaptedFilesIter>>;
