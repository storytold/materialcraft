pub(super) use craft_sbs::xml::{child_element, child_value, children_named, format_float, is_parameters_element, parse_floats, xml_escape};

#[cfg(test)]
mod shared_escape_tests {
    use super::xml_escape;

    #[test]
    fn sbs_attribute_wire_spelling_is_preserved() {
        assert_eq!(xml_escape("&<>\"'\t\n\r"), "&amp;&lt;&gt;&quot;&apos;&#9;&#10;&#13;");
        assert_eq!(xml_escape("\0\u{1f}\u{fffe}\u{ffff}é😀"), "é😀");
        assert_eq!(xml_escape("''"), "&apos;&apos;");
        assert_eq!(xml_escape(""), "");
    }

    #[test]
    fn written_attribute_roundtrips_valid_unicode_and_whitespace() {
        let value = "élan 😀 ' \" & < >\t\n\r";
        let xml = format!("<value v=\"{}\"/>", xml_escape(value));
        let doc = roxmltree::Document::parse(&xml).unwrap();
        assert_eq!(doc.root_element().attribute("v"), Some(value));
    }
}
