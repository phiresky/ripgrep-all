use crate::adapters::AdaptInfo;

// TODO: using iterator trait possible?? should basically be Iterator<AdaptInfo>
pub trait AdaptedFilesIter {
    // next takes a 'a-lived reference and returns an AdaptInfo that lives as long as the reference
    fn next<'a>(&'a mut self) -> Option<AdaptInfo<'a>>;
}

/// A single AdaptInfo
pub struct SingleAdaptedFileAsIter<'a> {
    ai: Option<AdaptInfo<'a>>,
}
impl SingleAdaptedFileAsIter<'_> {
    pub fn new<'a>(ai: AdaptInfo<'a>) -> SingleAdaptedFileAsIter<'a> {
        SingleAdaptedFileAsIter { ai: Some(ai) }
    }
}
impl AdaptedFilesIter for SingleAdaptedFileAsIter<'_> {
    fn next<'a>(&'a mut self) -> Option<AdaptInfo<'a>> {
        self.ai.take()
    }
}

pub type AdaptedFilesIterBox<'a> = Box<dyn AdaptedFilesIter + 'a>;
