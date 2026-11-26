/// An enum of named choices: serialized by id, listed in menu order, labelled for the UI. The
/// first choice is the default
macro_rules! choice {
    ($(#[$meta:meta])* $name:ident { $first:ident => $first_id:literal, $first_label:literal; $($variant:ident => $id:literal, $label:literal;)* }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
        pub enum $name {
            #[default]
            #[serde(rename = $first_id)]
            $first,
            $(#[serde(rename = $id)] $variant,)*
        }

        impl $name {
            /// Every choice, in menu order
            pub const ALL: &'static [$name] = &[$name::$first, $($name::$variant,)*];

            /// The id commands and saved states use
            pub fn id(self) -> &'static str {
                match self {
                    $name::$first => $first_id,
                    $($name::$variant => $id,)*
                }
            }

            /// The English label; the UI translates it
            pub fn label(self) -> &'static str {
                match self {
                    $name::$first => $first_label,
                    $($name::$variant => $label,)*
                }
            }

            /// The choice with this id
            pub fn from_id(id: &str) -> Option<$name> {
                $name::ALL.iter().copied().find(|choice| choice.id() == id)
            }
        }
    };
}
