use super::{LayoutFrame, SbsRead};
use crate::{FormatError, Result};

/// A validated SBS XML document that can supply layout and graph data without reparsing.
pub struct SbsDocument<'input> {
    document: craft_sbs::PackageDocument<'input>,
}

impl<'input> SbsDocument<'input> {
    /// Parse borrowed input with the same byte, XML-node, and DTD limits as the SBS reader.
    pub fn parse(bytes: &'input [u8]) -> Result<Self> {
        let document = craft_sbs::PackageDocument::parse(bytes).map_err(|error| FormatError::BadPackage(error.to_string()))?;
        Ok(Self { document })
    }

    /// Read the layout independently of whether its frames name library categories.
    pub fn layout(&self, uri: &str) -> Result<Vec<LayoutFrame>> {
        super::library_layout::read_document(self.document.document(), uri)
    }

    /// Apply one aggregate resource budget to inline data and the caller's linked reads.
    pub(crate) fn read_with_resource_budget(&self, name: &str, linked: &dyn Fn(&str) -> Option<Vec<u8>>, budget: &std::cell::Cell<u64>) -> Result<SbsRead> {
        super::read::read_document(&self.document, name, linked, Some(budget))
    }

    /// Read owned package data; the result does not retain the XML document or input bytes.
    pub fn read(&self, name: &str, linked: &dyn Fn(&str) -> Option<Vec<u8>>) -> Result<SbsRead> {
        super::read::read_document(&self.document, name, linked, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_data_outlives_the_shared_document() {
        let bytes = b"<package><content><graph><identifier v=\"Example\"/><compNodes/></graph></content></package>".to_vec();
        let document = SbsDocument::parse(&bytes).unwrap();

        let frames = document.layout("sbs://example.sbs").unwrap();
        let read = document.read("example.sbs", &|_| None).unwrap();
        drop(document);
        drop(bytes);

        assert!(frames.is_empty());
        assert_eq!(read.pkg.graphs.len(), 1);
        assert_eq!(read.pkg.graphs[0].name, "Example");
        assert!(read.warnings.is_empty());
    }

    #[test]
    fn malformed_documents_fail_before_either_reader_runs() {
        for bytes in [b"<package>".as_slice(), b"<other/>", b"\xff", b"<!DOCTYPE package [<!ENTITY x 'x'>]><package/>"] {
            assert!(SbsDocument::parse(bytes).is_err());
        }
    }
}
