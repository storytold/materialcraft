use super::{
    constant::{SELF_DEPENDENCY, SELF_DEPENDENCY_UID, UPDATER_VERSION},
    sbs_writer::SbsWriter,
};
use crate::Result;
use materialcraft_core::Package;
use std::collections::BTreeMap;

pub fn write_sbs(pkg: &Package) -> Result<Vec<u8>> {
    let format_version = pkg.format_version.as_deref().filter(|version| !version.trim().is_empty()).unwrap_or(UPDATER_VERSION);
    let mut writer = SbsWriter { last_uid: SELF_DEPENDENCY_UID, external_packages: BTreeMap::new() };
    let self_uid = SELF_DEPENDENCY_UID;

    let mut content = String::from("<content>");
    super::group_layout::write(&mut content, pkg, &mut writer, self_uid)?;
    content.push_str("</content>");

    let dependencies = std::iter::once((SELF_DEPENDENCY, self_uid)).chain(writer.external_packages.iter().map(|(file, uid)| (file.as_str(), *uid)));
    Ok(craft_sbs::write_package(&pkg.name, format_version, UPDATER_VERSION, dependencies, &content))
}
