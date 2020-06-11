use super::{FileAdapter, GetMetadata, ReadBox};
use anyhow::Result;
use std::io::Write;

// this trait / struct split is necessary because of "conflicting trait implementation" otherwise with SpawningFileAdapter
#[dyn_clonable::clonable]
pub trait WritingFileAdapterTrait: GetMetadata + Send + Clone {
    fn adapt_write(
        &self,
        a: super::AdaptInfo,
        detection_reason: &crate::matching::SlowMatcher,
        oup: &mut dyn Write,
    ) -> Result<()>;
}

pub struct WritingFileAdapter {
    inner: Box<dyn WritingFileAdapterTrait>,
}
impl WritingFileAdapter {
    pub fn new(inner: Box<dyn WritingFileAdapterTrait>) -> WritingFileAdapter {
        WritingFileAdapter { inner }
    }
}

impl GetMetadata for WritingFileAdapter {
    fn metadata(&self) -> &super::AdapterMeta {
        self.inner.metadata()
    }
}

impl FileAdapter for WritingFileAdapter {
    fn adapt(
        &self,
        a: super::AdaptInfo,
        detection_reason: &crate::matching::SlowMatcher,
    ) -> anyhow::Result<ReadBox> {
        let (r, w) = crate::pipe::pipe();
        let cc = self.inner.clone();
        let detc = detection_reason.clone();
        std::thread::spawn(move || {
            let mut oup = w;
            let ai = a;
            let res = cc.adapt_write(ai, &detc, &mut oup);
            if let Err(e) = res {
                oup.write_err(std::io::Error::new(std::io::ErrorKind::Other, e))
                    .expect("could not write err");
            }
        });

        Ok(Box::new(r))
    }
}
