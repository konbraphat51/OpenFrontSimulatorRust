// SPDX-License-Identifier: AGPL-3.0-only
//! Macro for string enums of the wire format that keep unknown values instead of failing.

/// Defines an enum over wire strings with an `Other(String)` fallback, plus `as_str`/`from_wire` and serde.
#[macro_export]
macro_rules! wire_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident = $wire:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, Eq)]
        pub enum $name {
            $(
                #[doc = $wire]
                $variant,
            )+
            /// A value this crate does not know.
            Other(String),
        }

        impl $name {
            /// The wire string.
            pub fn as_str(&self) -> &str {
                match self {
                    $(Self::$variant => $wire,)+
                    Self::Other(value) => value,
                }
            }

            /// Parses a wire string; unknown values become `Other`.
            pub fn from_wire(value: &str) -> Self {
                match value {
                    $($wire => Self::$variant,)+
                    other => Self::Other(other.to_string()),
                }
            }
        }

        impl serde::Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let value = String::deserialize(deserializer)?;
                Ok(Self::from_wire(&value))
            }
        }
    };
}
