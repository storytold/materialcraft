/// How a bitmap node interprets the channels stored in a package resource.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BitmapColorSpace {
    Srgb,
    Linear,
    FromFile,
}

impl BitmapColorSpace {
    pub fn from_index(index: i32) -> Self {
        match index {
            1 => Self::Linear,
            2 => Self::FromFile,
            _ => Self::Srgb,
        }
    }

    /// File-based reads have their own cache entry. Existing data and color requests use the shared texture format.
    pub fn resource_key(self, name: &str) -> String {
        match self {
            Self::FromFile => format!("\u{1}F{name}"),
            _ => craft_texture::resource_key(name, self == Self::Linear),
        }
    }

    pub fn split_resource_key(key: &str) -> (&str, Self) {
        if let Some(name) = key.strip_prefix("\u{1}F") {
            (name, Self::FromFile)
        } else {
            let (name, linear) = craft_texture::split_resource_key(key);
            (name, if linear { Self::Linear } else { Self::Srgb })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::BitmapColorSpace;

    #[test]
    fn bitmap_resource_keys_preserve_package_paths_and_literal_prefixes() {
        for name in ["field", "\u{1}Ffield", "\u{1}Lfield", "child.sbs\0field"] {
            for space in [BitmapColorSpace::FromFile, BitmapColorSpace::Srgb, BitmapColorSpace::Linear] {
                let key = space.resource_key(name);
                assert_eq!(BitmapColorSpace::split_resource_key(&key), (name, space));
            }
        }
    }
}
