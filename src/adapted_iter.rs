use crate::adapters::AdaptInfo;

// TODO: using iterator trait possible?? should basically be Iterator<AdaptInfo>
pub trait AdaptedFilesIter: Send {
    // next takes a 'a-lived reference and returns an AdaptInfo that lives as long as the reference
    fn next<'a>(&'a mut self) -> Option<AdaptInfo>;
}

/// A single AdaptInfo
pub struct SingleAdaptedFileAsIter {
    ai: Option<AdaptInfo>,
}
impl SingleAdaptedFileAsIter {
    pub fn new<'a>(ai: AdaptInfo) -> SingleAdaptedFileAsIter {
        SingleAdaptedFileAsIter { ai: Some(ai) }
    }
}
impl AdaptedFilesIter for SingleAdaptedFileAsIter {
    fn next<'a>(&'a mut self) -> Option<AdaptInfo> {
        self.ai.take()
    }
}

pub type AdaptedFilesIterBox = Box<dyn AdaptedFilesIter>;
