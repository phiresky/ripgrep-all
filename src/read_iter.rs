use crate::adapters::AdaptInfo;

// TODO: using iterator trait possible?? should basically be Iterator<AdaptInfo>
pub trait ReadIter {
    // next takes a 'a-lived reference and returns an AdaptInfo that lives as long as the reference
    fn next<'a>(&'a mut self) -> Option<AdaptInfo<'a>>;
}

/// A single AdaptInfo
pub struct SingleReadIter<'a> {
    ai: Option<AdaptInfo<'a>>,
}
impl SingleReadIter<'_> {
    pub fn new<'a>(ai: AdaptInfo<'a>) -> SingleReadIter<'a> {
        SingleReadIter { ai: Some(ai) }
    }
}
impl ReadIter for SingleReadIter<'_> {
    fn next<'a>(&'a mut self) -> Option<AdaptInfo<'a>> {
        self.ai.take()
    }
}

pub type ReadIterBox<'a> = Box<dyn ReadIter + 'a>;
