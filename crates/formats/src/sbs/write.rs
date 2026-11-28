use super::{
    constant::{SELF_DEPENDENCY, SELF_DEPENDENCY_UID, UPDATER_VERSION},
    sbs_writer::SbsWriter,
};
use crate::Result;
use materialcraft_core::Package;
use std::collections::BTreeMap;

/// Schema revision to declare when writing a package, including packages with older app-release headers.
pub fn output_format_version(imported: Option<&str>) -> &str {
    const SCHEMA: &str = "1.1.0.202401";
    let Some(version) = imported.filter(|v| !v.trim().is_empty()) else { return SCHEMA };
    // Earlier saves put the three-part app release in this field. Readers use it to select the XML grammar.
    let mut parts = version.split('.');
    let is_release = (0..3).all(|_| parts.next().is_some_and(|p| !p.is_empty() && p.bytes().all(|c| c.is_ascii_digit()))) && parts.next().is_none();
    if is_release { SCHEMA } else { version }
}

pub fn write_sbs(pkg: &Package) -> Result<Vec<u8>> {
    let format_version = output_format_version(pkg.format_version.as_deref());
    let mut writer = SbsWriter { last_uid: SELF_DEPENDENCY_UID, external_packages: BTreeMap::new() };

    let mut content = String::from("<content>");
    super::group_layout::write(&mut content, pkg, &mut writer, SELF_DEPENDENCY_UID)?;
    content.push_str("</content>");

    let dependencies = std::iter::once((SELF_DEPENDENCY, SELF_DEPENDENCY_UID)).chain(writer.external_packages.iter().map(|(file, uid)| (file.as_str(), *uid)));
    Ok(craft_sbs::write_package(&pkg.name, format_version, UPDATER_VERSION, dependencies, &content))
}
